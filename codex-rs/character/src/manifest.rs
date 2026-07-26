// Modified by Heiervang Technologies.
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::config_types::ModeKind;
use serde::Deserialize;
use serde::Serialize;

use crate::AvatarPackErrorKind;
use crate::CHARACTER_SCHEMA_VERSION;
use crate::validate_avatar_selector;

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
    pub storage_id: Option<String>,
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
    collision_errors: Vec<ValidationIssue>,
}

impl CharacterCatalog {
    pub fn load(codex_home: &Path) -> Self {
        Self::load_registry(codex_home.join("characters"))
    }

    pub fn load_registry(registry_root: PathBuf) -> Self {
        let mut packages = Vec::new();
        if let Ok(entries) = fs::read_dir(&registry_root) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                    packages.push(entry.path());
                }
            }
        }
        packages.sort();

        let entries = packages
            .iter()
            .map(|package| {
                let storage_id = package
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string);
                validate_manifest_path_for_storage(
                    &package.join(CHARACTER_MANIFEST_FILE),
                    storage_id,
                )
            })
            .collect::<Vec<_>>();
        let mut errors = entries
            .iter()
            .flat_map(|report| report.errors.iter().cloned())
            .collect::<Vec<_>>();
        let collision_errors = collision_errors(&entries);
        errors.extend(collision_errors.iter().cloned());
        Self {
            registry_root,
            entries,
            errors,
            collision_errors,
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
        if !self.collision_errors.is_empty() {
            return Err(self.collision_errors.clone());
        }
        let exact = self
            .entries
            .iter()
            .find(|report| {
                report.storage_id.as_deref() == Some(input) || report.id() == Some(input)
            })
            .map(|report| (report, MatchKind::ExactCanonical));
        let casefold = self
            .entries
            .iter()
            .find(|report| {
                report
                    .storage_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(input))
                    || report.id().is_some_and(|id| id.eq_ignore_ascii_case(input))
            })
            .map(|report| (report, MatchKind::CasefoldCanonical));
        let alias = self
            .entries
            .iter()
            .find(|report| {
                report.manifest.as_ref().is_some_and(|manifest| {
                    manifest
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(input))
                })
            })
            .map(|report| (report, MatchKind::ExplicitAlias));
        let Some((report, match_kind)) = exact.or(casefold).or(alias) else {
            return Err(vec![ValidationIssue::new(
                ValidationIssueCode::NotFound,
                format!("no character matches {input:?}"),
            )]);
        };
        if !report.errors.is_empty() {
            return Err(report.errors.clone());
        }
        let Some(manifest) = report.manifest.as_ref() else {
            return Err(vec![ValidationIssue::new(
                ValidationIssueCode::InvalidManifest,
                "character report has no parsed manifest",
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
}

pub fn validate_manifest_path(path: &Path) -> ValidationReport {
    validate_manifest_path_for_storage(path, inferred_storage_id(path))
}

fn validate_manifest_path_for_storage(path: &Path, storage_id: Option<String>) -> ValidationReport {
    let mut report = ValidationReport {
        manifest_path: path.to_path_buf(),
        storage_id,
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
    if report
        .storage_id
        .as_deref()
        .is_some_and(|storage_id| storage_id != manifest.id)
    {
        report.errors.push(
            ValidationIssue::new(
                ValidationIssueCode::InvalidManifest,
                format!(
                    "manifest id {:?} must match character package directory {:?}",
                    manifest.id,
                    report.storage_id.as_deref().unwrap_or_default()
                ),
            )
            .at_path("id"),
        );
    }
    if let Some(package_root) = path.parent() {
        validate_avatar_packs(package_root, &manifest, &mut report.errors);
    }
    report.manifest = Some(manifest);
    report
}

fn inferred_storage_id(path: &Path) -> Option<String> {
    let package = path.parent()?;
    if package.parent()?.file_name()?.to_str()? != "characters" {
        return None;
    }
    package.file_name()?.to_str().map(str::to_string)
}

fn validate_avatar_packs(
    package_root: &Path,
    manifest: &CharacterManifestV1,
    errors: &mut Vec<ValidationIssue>,
) {
    if !manifest.avatar.as_str().trim().is_empty()
        && is_safe_relative_selector(manifest.avatar.as_str())
        && let Err(error) = validate_avatar_selector(package_root, &manifest.avatar)
    {
        let code = if error.kind == AvatarPackErrorKind::MissingManifest {
            ValidationIssueCode::MissingAvatar
        } else {
            ValidationIssueCode::InvalidManifest
        };
        errors.push(ValidationIssue::new(code, error.message).at_path("avatar"));
    }
    let mut mode_avatars = manifest.avatar_by_mode.iter().collect::<Vec<_>>();
    mode_avatars.sort_by_key(|(mode, _)| mode_key(**mode));
    for (mode, selector) in mode_avatars {
        if !selector.as_str().trim().is_empty()
            && is_safe_relative_selector(selector.as_str())
            && let Err(error) = validate_avatar_selector(package_root, selector)
        {
            errors.push(
                ValidationIssue::new(ValidationIssueCode::InvalidManifest, error.message)
                    .at_path(format!("avatarByMode.{}", mode_key(*mode))),
            );
        }
    }
}

fn mode_key(mode: ModeKind) -> &'static str {
    match mode {
        ModeKind::Plan => "plan",
        ModeKind::Larp => "larp",
        ModeKind::LockedIn => "locked_in",
        ModeKind::Based => "based",
        ModeKind::Cringe => "cringe",
        ModeKind::Ultrachill => "ultrachill",
        ModeKind::PairProgramming => "pair_programming",
        ModeKind::Execute => "execute",
    }
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
    let mut mode_avatars = manifest.avatar_by_mode.iter().collect::<Vec<_>>();
    mode_avatars.sort_by_key(|(mode, _)| mode_key(**mode));
    for (mode, selector) in mode_avatars {
        if selector.as_str().trim().is_empty() || !is_safe_relative_selector(selector.as_str()) {
            errors.push(
                ValidationIssue::new(
                    ValidationIssueCode::InvalidManifest,
                    "avatarByMode values must be safe relative character-package paths",
                )
                .at_path(format!("avatarByMode.{}", mode_key(*mode))),
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
    let parsed = entries
        .iter()
        .filter_map(|report| report.manifest.as_ref().map(|manifest| (report, manifest)))
        .collect::<Vec<_>>();
    let mut canonical_owners = BTreeMap::<String, Vec<&ValidationReport>>::new();
    let mut alias_owners = BTreeMap::<String, Vec<(&ValidationReport, &str)>>::new();
    for (report, manifest) in &parsed {
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
