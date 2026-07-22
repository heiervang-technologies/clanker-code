use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::config_types::ModeKind;
use serde::Deserialize;
use serde::Serialize;

use crate::CHARACTER_SCHEMA_VERSION;

const CHARACTER_MANIFEST_FILE: &str = "character.json";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AvatarSelector(pub String);

impl AvatarSelector {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AvatarPlacement {
    FarLeft,
    #[default]
    FarRight,
    AboveLeft,
    AboveCenter,
    AboveRight,
    BelowLeft,
    BelowCenter,
    BelowRight,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterManifestV1 {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub avatar: AvatarSelector,
    #[serde(default)]
    pub avatar_by_mode: HashMap<ModeKind, AvatarSelector>,
    #[serde(default)]
    pub avatar_placement: AvatarPlacement,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub voice_profile: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationIssueCode {
    InvalidManifest,
    InvalidId,
    MissingAvatar,
    CanonicalCollision,
    AliasCollision,
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    CanonicalVsCanonical,
    AliasVsCanonical,
    AliasVsAlias,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub code: ValidationIssueCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_kind: Option<ConflictKind>,
}

impl ValidationIssue {
    fn new(code: ValidationIssueCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            conflict_kind: None,
        }
    }

    fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    fn with_conflict_kind(mut self, conflict_kind: ConflictKind) -> Self {
        self.conflict_kind = Some(conflict_kind);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    pub manifest_path: PathBuf,
    pub manifest: Option<CharacterManifestV1>,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn id(&self) -> Option<&str> {
        self.manifest.as_ref().map(|manifest| manifest.id.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    ExactCanonical,
    CasefoldCanonical,
    ExplicitAlias,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCharacter {
    pub input: String,
    pub manifest: CharacterManifestV1,
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub match_kind: MatchKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterCatalog {
    registry_root: PathBuf,
    entries: Vec<ValidationReport>,
    errors: Vec<ValidationIssue>,
}

impl CharacterCatalog {
    pub fn load(codex_home: &Path) -> Self {
        Self::load_registry(codex_home.join("characters"))
    }

    pub fn load_registry(registry_root: PathBuf) -> Self {
        let mut manifest_paths = Vec::new();
        if let Ok(entries) = fs::read_dir(&registry_root) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                    let manifest_path = entry.path().join(CHARACTER_MANIFEST_FILE);
                    if manifest_path.is_file() {
                        manifest_paths.push(manifest_path);
                    }
                }
            }
        }
        manifest_paths.sort();

        let entries = manifest_paths
            .iter()
            .map(|path| validate_manifest_path(path))
            .collect::<Vec<_>>();
        let mut errors = entries
            .iter()
            .flat_map(|report| report.errors.iter().cloned())
            .collect::<Vec<_>>();
        errors.extend(collision_errors(&entries));
        Self {
            registry_root,
            entries,
            errors,
        }
    }

    pub fn registry_root(&self) -> &Path {
        &self.registry_root
    }

    pub fn entries(&self) -> &[ValidationReport] {
        &self.entries
    }

    pub fn errors(&self) -> &[ValidationIssue] {
        &self.errors
    }

    pub fn resolve(&self, input: &str) -> Result<ResolvedCharacter, Vec<ValidationIssue>> {
        if !self.errors.is_empty() {
            return Err(self.errors.clone());
        }
        let manifests = self.valid_manifests();
        let exact = manifests
            .iter()
            .find(|(_, manifest)| manifest.id == input)
            .map(|entry| (*entry, MatchKind::ExactCanonical));
        let casefold = manifests
            .iter()
            .find(|(_, manifest)| manifest.id.eq_ignore_ascii_case(input))
            .map(|entry| (*entry, MatchKind::CasefoldCanonical));
        let alias = manifests
            .iter()
            .find(|(_, manifest)| {
                manifest
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(input))
            })
            .map(|entry| (*entry, MatchKind::ExplicitAlias));
        let Some(((report, manifest), match_kind)) = exact.or(casefold).or(alias) else {
            return Err(vec![ValidationIssue::new(
                ValidationIssueCode::NotFound,
                format!("no character matches {input:?}"),
            )]);
        };
        let manifest_path = report.manifest_path.clone();
        let package_root = manifest_path
            .parent()
            .unwrap_or(self.registry_root.as_path())
            .to_path_buf();
        Ok(ResolvedCharacter {
            input: input.to_string(),
            manifest: manifest.clone(),
            manifest_path,
            package_root,
            match_kind,
        })
    }

    fn valid_manifests(&self) -> Vec<(&ValidationReport, &CharacterManifestV1)> {
        self.entries
            .iter()
            .filter(|report| report.is_valid())
            .filter_map(|report| report.manifest.as_ref().map(|manifest| (report, manifest)))
            .collect()
    }
}

pub fn validate_manifest_path(path: &Path) -> ValidationReport {
    let mut report = ValidationReport {
        manifest_path: path.to_path_buf(),
        manifest: None,
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            report.errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::InvalidManifest,
                    format!("failed to read manifest: {error}"),
                )
                .at_path(path.display().to_string()),
            );
            return report;
        }
    };
    let manifest = match serde_json::from_str::<CharacterManifestV1>(&contents) {
        Ok(manifest) => manifest,
        Err(error) => {
            report.errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::InvalidManifest,
                    format!("failed to parse manifest: {error}"),
                )
                .at_path(path.display().to_string()),
            );
            return report;
        }
    };
    validate_manifest_fields(&manifest, &mut report.errors);
    report.manifest = Some(manifest);
    report
}

fn validate_manifest_fields(manifest: &CharacterManifestV1, errors: &mut Vec<ValidationIssue>) {
    if manifest.schema_version != CHARACTER_SCHEMA_VERSION {
        errors.push(
            ValidationIssue::new(
                ValidationIssueCode::InvalidManifest,
                format!(
                    "schemaVersion must be {CHARACTER_SCHEMA_VERSION}, got {}",
                    manifest.schema_version
                ),
            )
            .at_path("schemaVersion"),
        );
    }
    if !is_valid_id(&manifest.id) {
        errors.push(
            ValidationIssue::new(
                ValidationIssueCode::InvalidId,
                "id must be a lowercase portable slug",
            )
            .at_path("id"),
        );
    }
    if manifest.display_name.trim().is_empty() {
        errors.push(
            ValidationIssue::new(
                ValidationIssueCode::InvalidManifest,
                "displayName must not be empty",
            )
            .at_path("displayName"),
        );
    }
    if manifest.avatar.as_str().trim().is_empty() {
        errors.push(
            ValidationIssue::new(ValidationIssueCode::MissingAvatar, "avatar is required")
                .at_path("avatar"),
        );
    } else if !is_safe_relative_selector(manifest.avatar.as_str()) {
        errors.push(
            ValidationIssue::new(
                ValidationIssueCode::InvalidManifest,
                "avatar must be a safe relative character-package path",
            )
            .at_path("avatar"),
        );
    }
    for (mode, selector) in &manifest.avatar_by_mode {
        if selector.as_str().trim().is_empty() || !is_safe_relative_selector(selector.as_str()) {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::InvalidManifest,
                    "avatarByMode values must be safe relative character-package paths",
                )
                .at_path(format!("avatarByMode.{}", mode.display_name())),
            );
        }
    }
    for (index, alias) in manifest.aliases.iter().enumerate() {
        if alias.trim().is_empty() {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::InvalidManifest,
                    "aliases must not be empty",
                )
                .at_path(format!("aliases.{index}")),
            );
        }
    }
}

fn collision_errors(entries: &[ValidationReport]) -> Vec<ValidationIssue> {
    let valid = entries
        .iter()
        .filter(|report| report.is_valid())
        .filter_map(|report| report.manifest.as_ref().map(|manifest| (report, manifest)))
        .collect::<Vec<_>>();
    let mut canonical_owners = BTreeMap::<String, Vec<&ValidationReport>>::new();
    let mut alias_owners = BTreeMap::<String, Vec<(&ValidationReport, &str)>>::new();
    for (report, manifest) in &valid {
        canonical_owners
            .entry(manifest.id.to_ascii_lowercase())
            .or_default()
            .push(report);
        for alias in &manifest.aliases {
            alias_owners
                .entry(alias.to_ascii_lowercase())
                .or_default()
                .push((report, alias));
        }
    }

    let mut errors = Vec::new();
    for (canonical, owners) in &canonical_owners {
        if owners.len() > 1 {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::CanonicalCollision,
                    format!("canonical id {canonical:?} is declared by multiple manifests"),
                )
                .at_path(owners[1].manifest_path.display().to_string())
                .with_conflict_kind(ConflictKind::CanonicalVsCanonical),
            );
        }
    }
    for (alias, owners) in &alias_owners {
        if owners.len() > 1 {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::AliasCollision,
                    format!("alias {alias:?} is declared by multiple manifests"),
                )
                .at_path(owners[1].0.manifest_path.display().to_string())
                .with_conflict_kind(ConflictKind::AliasVsAlias),
            );
        }
    }
    for (alias, owners) in &alias_owners {
        if canonical_owners.contains_key(alias) {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::AliasCollision,
                    format!("alias {alias:?} conflicts with a canonical id"),
                )
                .at_path(owners[0].0.manifest_path.display().to_string())
                .with_conflict_kind(ConflictKind::AliasVsCanonical),
            );
        }
    }
    errors
}

fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !id.contains("--")
}

fn is_safe_relative_selector(selector: &str) -> bool {
    let path = Path::new(selector);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
