//! Lower-level contracts for the external character wizard and activation journal.
//!
//! This module deliberately has no TUI or state-database dependency. Callers provide package
//! bytes and an implementation of the last-active transaction boundary.

use std::fs;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use crate::validate_canonical_id;

pub const WIZARD_PROTOCOL_VERSION: u32 = 1;
const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CharacterWizardRequestV1 {
    pub protocol_version: u32,
    pub operation: WizardOperation,
    pub operation_id: WizardOperationId,
    /// The process sees only `.`; the caller sets its cwd to the private operation root.
    pub operation_root: String,
    pub canonical_id: Option<String>,
    pub display_name: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WizardOperation {
    Create,
    Import,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CharacterWizardResultV1 {
    pub protocol_version: u32,
    pub outcome: WizardOutcome,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WizardOutcome {
    Success { candidate_path: String },
    Cancelled,
    Error { message: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct WizardOperationReceipt {
    pub operation_id: WizardOperationId,
    pub expected_prior: Option<String>,
    pub target: String,
    pub committed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct WizardOperationId(String);

impl WizardOperationId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    pub fn parse(value: impl Into<String>) -> io::Result<Self> {
        let value = value.into();
        if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "operation id must be 32 lowercase hexadecimal characters"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl CharacterWizardResultV1 {
    pub fn decode_bounded(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_JSON_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "wizard result is too large"));
        }
        let result: Self = serde_json::from_slice(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if result.protocol_version != WIZARD_PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported wizard result protocol version",
            ));
        }
        match &result.outcome {
            WizardOutcome::Success { candidate_path } => {
                if candidate_path.is_empty() || candidate_path.len() > 4096 || candidate_path.chars().any(char::is_control) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid wizard candidate"));
                }
            }
            WizardOutcome::Error { message } => {
                if message.is_empty() || message.len() > 4096 || message.chars().any(char::is_control) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid wizard error"));
                }
            }
            WizardOutcome::Cancelled => {}
        }
        Ok(result)
    }
}

pub fn encode_bounded_request(request: &CharacterWizardRequestV1) -> io::Result<Vec<u8>> {
    if request.protocol_version != WIZARD_PROTOCOL_VERSION || request.operation_root != "." {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid wizard request"));
    }
    if request.operation_id.as_str().len() != 32 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid operation id")); }
    for value in request.canonical_id.iter().chain(request.display_name.iter()).chain(request.aliases.iter()) {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid wizard metadata"));
        }
    }
    if matches!(request.operation, WizardOperation::Create) && request.canonical_id.is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "create requires a canonical id"));
    }
    let bytes = serde_json::to_vec(request).map_err(io::Error::other)?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "wizard request is too large"));
    }
    Ok(bytes)
}

pub fn resolve_wizard_executable(provider_root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.is_absolute() || !is_safe_relative(relative) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wizard executable escapes provider"));
    }
    let root = fs::canonicalize(provider_root)?;
    let candidate = fs::canonicalize(root.join(relative))?;
    if !candidate.starts_with(&root) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wizard executable escapes provider"));
    }
    let metadata = fs::metadata(&candidate)?;
    if !metadata.is_file() || !is_executable(&metadata) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wizard executable is not an ordinary executable file"));
    }
    Ok(candidate)
}

pub fn resolve_candidate_path(operation_root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.is_absolute() || !is_safe_relative(relative) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wizard candidate escapes operation"));
    }
    let root = fs::canonicalize(operation_root)?;
    let candidate = fs::canonicalize(root.join(relative))?;
    if !candidate.starts_with(&root) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wizard candidate escapes operation"));
    }
    Ok(candidate)
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(component, Component::Normal(name) if !name.is_empty() && name.to_str().is_some_and(|value| !value.chars().any(char::is_control)))
        })
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPhase {
    Prepared,
    PackageInstalled,
    StateCommitted,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ActivationJournal {
    pub schema_version: u32,
    pub operation_id: WizardOperationId,
    pub canonical_id: String,
    pub prior_last_active: Option<String>,
    pub phase: ActivationPhase,
}

pub trait LastActiveStore {
    type Error: std::error::Error + Send + Sync + 'static;

    fn commit_activation(
        &mut self,
        operation_id: &str,
        expected_prior: Option<&str>,
        canonical_id: &str,
    ) -> Result<WizardOperationReceipt, Self::Error>;
    fn activation_status(&self, operation_id: &str) -> Result<Option<WizardOperationReceipt>, Self::Error>;
}

pub struct ActivationJournalCoordinator<S> {
    characters_root: PathBuf,
    store: S,
}

impl<S: LastActiveStore> ActivationJournalCoordinator<S> {
    pub fn new(characters_root: PathBuf, store: S) -> Self {
        Self { characters_root, store }
    }

    pub fn journal_path(&self, operation_id: &WizardOperationId) -> io::Result<PathBuf> {
        Ok(self.characters_root.join(".staging").join(format!("{}.json", operation_id.as_str())))
    }

    pub fn write_journal(&self, journal: &ActivationJournal) -> io::Result<()> {
        if !validate_canonical_id(&journal.canonical_id) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid canonical id")); }
        if journal.schema_version != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unsupported activation journal"));
        }
        let staging = self.characters_root.join(".staging");
        fs::create_dir_all(&staging)?;
        let path = self.journal_path(&journal.operation_id)?;
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(journal).map_err(io::Error::other)?;
        let mut file = File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        File::open(&staging)?.sync_all()?;
        Ok(())
    }

    pub fn commit_last_active(
        &mut self,
        journal: &ActivationJournal,
    ) -> Result<WizardOperationReceipt, ActivationError<S::Error>> {
        let receipt = self.store
            .commit_activation(
                journal.operation_id.as_str(),
                journal.prior_last_active.as_deref(),
                &journal.canonical_id,
            )
            .map_err(ActivationError::Store)?;
        validate_receipt(&receipt, journal)?;
        Ok(receipt)
    }

    pub fn state_is_committed(
        &self,
        journal: &ActivationJournal,
    ) -> Result<Option<WizardOperationReceipt>, ActivationError<S::Error>> {
        let receipt = self.store.activation_status(journal.operation_id.as_str()).map_err(ActivationError::Store)?;
        if let Some(receipt) = &receipt {
            validate_receipt::<S::Error>(receipt, journal)?;
        }
        Ok(receipt)
    }
}

pub trait CharacterPackageSource {
    fn copy_into(&self, destination: &Path) -> io::Result<()>;
}

pub struct DirectoryPackageSource<'a> {
    pub source: &'a Path,
}

impl CharacterPackageSource for DirectoryPackageSource<'_> {
    fn copy_into(&self, destination: &Path) -> io::Result<()> {
        copy_package_tree(self.source, destination, 0, &mut 0, &mut 0)
    }
}

fn copy_package_tree(source: &Path, destination: &Path, depth: usize, files: &mut usize, bytes: &mut u64) -> io::Result<()> {
    if depth > 8 { return Err(io::Error::new(io::ErrorKind::InvalidData, "package nesting is too deep")); }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "package contains unsafe entry"));
        }
        let target = destination.join(entry.file_name());
        if kind.is_dir() { copy_package_tree(&entry.path(), &target, depth + 1, files, bytes)?; }
        else {
            *files += 1;
            let metadata = entry.metadata()?;
            *bytes = (*bytes).saturating_add(metadata.len());
            if *files > 256 || *bytes > 16 * 1024 * 1024 { return Err(io::Error::new(io::ErrorKind::InvalidData, "package exceeds bounds")); }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn validate_staged_package(package_root: &Path, canonical_id: &str) -> io::Result<()> {
    if !validate_canonical_id(canonical_id) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid canonical id")); }
    let report = crate::validate_manifest_path(&package_root.join("character.json"));
    if !report.is_valid() { return Err(io::Error::new(io::ErrorKind::InvalidData, "character package is invalid")); }
    Ok(())
}

pub fn stage_character_package<S: CharacterPackageSource>(
    characters_root: &Path,
    operation_id: &str,
    canonical_id: &str,
    source: &S,
) -> io::Result<PathBuf> {
    let operation_id = WizardOperationId::parse(operation_id)?;
    if !validate_canonical_id(canonical_id) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid canonical id")); }
    let operation_dir = characters_root.join(".staging").join(operation_id.as_str());
    fs::create_dir_all(characters_root.join(".staging"))?;
    fs::create_dir(&operation_dir)?;
    let lock = operation_dir.join(".lock");
    if let Err(error) = File::options().write(true).create_new(true).open(&lock)
        .and_then(|_| fs::create_dir(operation_dir.join(canonical_id)))
        .and_then(|_| source.copy_into(&operation_dir.join(canonical_id)))
        .and_then(|_| validate_staged_package(&operation_dir.join(canonical_id), canonical_id)) {
        if let Err(cleanup) = fs::remove_dir_all(&operation_dir) {
            return Err(io::Error::other(format!("{error}; cleanup failed: {cleanup}")));
        }
        return Err(error);
    }
    Ok(operation_dir.join(canonical_id))
}

fn validate_receipt<E: std::error::Error + 'static>(receipt: &WizardOperationReceipt, journal: &ActivationJournal) -> Result<(), ActivationError<E>> {
    if !receipt.committed
        || receipt.operation_id != journal.operation_id
        || receipt.expected_prior != journal.prior_last_active
        || receipt.target != journal.canonical_id
    {
        return Err(ActivationError::InvalidReceipt);
    }
    Ok(())
}

fn validate_segment(value: &str) -> io::Result<()> {
    if value.is_empty() || value == "." || value == ".." || value.contains('/') || value.contains('\\') || value.chars().any(char::is_control) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid operation or character id"));
    }
    Ok(())
}

#[derive(Debug)]
pub enum ActivationError<E: std::error::Error + 'static> {
    Store(E),
    InvalidReceipt,
}

impl<E: std::error::Error + 'static> std::fmt::Display for ActivationError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "last-active transaction failed: {error}"),
            Self::InvalidReceipt => formatter.write_str("last-active receipt does not match operation"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ActivationError<E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::tempdir;

    #[test]
    fn request_forces_private_operation_root() {
        let request = CharacterWizardRequestV1 {
            protocol_version: 1,
            operation: WizardOperation::Create,
            operation_id: WizardOperationId::parse("0123456789abcdef0123456789abcdef").unwrap(),
            operation_root: ".".into(),
            canonical_id: Some("rusty".into()),
            display_name: Some("Rusty".into()),
            aliases: vec![],
        };
        assert!(encode_bounded_request(&request).is_ok());
        let mut invalid = request;
        invalid.operation_root = "/private/home".into();
        assert!(encode_bounded_request(&invalid).is_err());
    }

    #[test]
    fn operation_ids_are_generated_unique_and_canonical() {
        let first = WizardOperationId::generate();
        let second = WizardOperationId::generate();
        assert_ne!(first, second);
        assert_eq!(first.as_str().len(), 32);
        assert_eq!(WizardOperationId::parse(first.as_str()).unwrap(), first);
        assert!(WizardOperationId::parse(&first.as_str().to_uppercase()).is_err());
        assert!(WizardOperationId::parse("not-an-operation-id").is_err());
    }

    #[test]
    fn result_decode_is_versioned_and_bounded() {
        let result = CharacterWizardResultV1 {
            protocol_version: 1,
            outcome: WizardOutcome::Success { candidate_path: "candidate".into() },
        };
        let bytes = serde_json::to_vec(&result).unwrap();
        assert_eq!(CharacterWizardResultV1::decode_bounded(&bytes).unwrap(), result);
        assert!(CharacterWizardResultV1::decode_bounded(&[b'x'; MAX_JSON_BYTES + 1]).is_err());
    }

    #[test]
    fn candidate_rejects_traversal_and_symlink_escape() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("candidate"), b"x").unwrap();
        fs::create_dir_all(root.path().join("nested")).unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink(outside.path().join("candidate"), root.path().join("nested/link")).unwrap();
        assert!(resolve_candidate_path(root.path(), Path::new("../candidate")).is_err());
        #[cfg(unix)] assert!(resolve_candidate_path(root.path(), Path::new("nested/link")).is_err());
    }

    #[test]
    fn journal_uses_derived_operation_path_and_rejects_bad_versions() {
        let root = tempdir().unwrap();
        let coordinator = ActivationJournalCoordinator::new(root.path().to_path_buf(), FakeStore::default());
        let operation_id = WizardOperationId::parse("0123456789abcdef0123456789abcdef").unwrap();
        let journal = ActivationJournal { schema_version: 1, operation_id: operation_id.clone(), canonical_id: "rusty".into(), prior_last_active: None, phase: ActivationPhase::Prepared };
        coordinator.write_journal(&journal).unwrap();
        assert_eq!(coordinator.journal_path(&operation_id).unwrap(), root.path().join(".staging/0123456789abcdef0123456789abcdef.json"));
        let invalid = ActivationJournal { schema_version: 2, ..journal };
        assert!(coordinator.write_journal(&invalid).is_err());
    }

    #[derive(Default)]
    struct FakeStore { committed: HashSet<String> }

    impl LastActiveStore for FakeStore {
        type Error = io::Error;
        fn commit_activation(&mut self, operation_id: &str, expected_prior: Option<&str>, canonical_id: &str) -> Result<WizardOperationReceipt, Self::Error> {
            self.committed.insert(operation_id.into());
            Ok(WizardOperationReceipt { operation_id: WizardOperationId::parse(operation_id).unwrap(), expected_prior: expected_prior.map(str::to_string), target: canonical_id.into(), committed: true })
        }
        fn activation_status(&self, operation_id: &str) -> Result<Option<WizardOperationReceipt>, Self::Error> { Ok(self.committed.contains(operation_id).then(|| WizardOperationReceipt { operation_id: WizardOperationId::parse(operation_id).unwrap(), expected_prior: None, target: "rusty".into(), committed: true })) }
    }
}
