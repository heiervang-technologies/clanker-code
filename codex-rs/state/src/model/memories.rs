use chrono::DateTime;
use chrono::Utc;
use codex_character::CharacterCatalog;
use codex_character::MatchKind;
use codex_git_utils::canonicalize_git_remote_url;
use codex_protocol::ThreadId;
use sha2::Digest;
use sha2::Sha256;
use std::fmt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use super::ThreadMetadata;

const MEMORY_PROJECT_KEY_VERSION: u8 = 1;

/// Canonical character identifier accepted by memory storage APIs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalClankerId(String);

impl CanonicalClankerId {
    /// Proves that `id` is the exact canonical id resolved by this catalog.
    pub fn resolve_exact(catalog: &CharacterCatalog, id: &str) -> Result<Self, MemoryScopeError> {
        let resolved = catalog
            .resolve(id)
            .map_err(|_| MemoryScopeError::UnverifiedCanonicalClankerId { id: id.to_string() })?;
        if resolved.match_kind != MatchKind::ExactCanonical || resolved.manifest.id != id {
            return Err(MemoryScopeError::UnverifiedCanonicalClankerId { id: id.to_string() });
        }
        Self::from_stored(id.to_string())
    }

    pub(crate) fn from_stored(id: impl Into<String>) -> Result<Self, MemoryScopeError> {
        let id = id.into();
        let bytes = id.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= 64
            && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
            && !id.contains("--");
        if !valid {
            return Err(MemoryScopeError::InvalidCanonicalClankerId { id });
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for CanonicalClankerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable project identity used to partition character memories.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryProjectKey(String);

impl MemoryProjectKey {
    pub fn from_git_origin(origin: &str) -> Result<Self, MemoryScopeError> {
        if origin.chars().any(char::is_control) {
            return Err(MemoryScopeError::InvalidProjectOrigin);
        }
        let origin = origin
            .find(['?', '#'])
            .map_or(origin, |suffix_index| &origin[..suffix_index]);
        let canonical =
            canonicalize_git_remote_url(origin).ok_or(MemoryScopeError::InvalidProjectOrigin)?;
        Ok(Self(format!(
            "v{MEMORY_PROJECT_KEY_VERSION}:git:{canonical}"
        )))
    }

    pub fn from_canonical_path(path: impl Into<PathBuf>) -> Result<Self, MemoryScopeError> {
        let path = path.into();
        let Some(path_text) = path.to_str() else {
            return Err(MemoryScopeError::InvalidProjectPath);
        };
        let normalized = path.components().collect::<PathBuf>();
        let has_forbidden_text = path_text.chars().any(char::is_control)
            || path_text
                .split(['/', '\\'])
                .any(|component| matches!(component, "." | ".."));
        let mut saw_prefix = false;
        let mut saw_root = false;
        let valid_components = path.components().all(|component| match component {
            Component::Prefix(_) if !saw_prefix && !saw_root => {
                saw_prefix = true;
                true
            }
            Component::RootDir if !saw_root => {
                saw_root = true;
                true
            }
            Component::Normal(_) if saw_root => true,
            _ => false,
        });
        if !path.is_absolute()
            || !saw_root
            || normalized.as_os_str() != path.as_os_str()
            || has_forbidden_text
            || !valid_components
        {
            return Err(MemoryScopeError::InvalidProjectPath);
        }
        Ok(Self(format!(
            "v{MEMORY_PROJECT_KEY_VERSION}:path:{}",
            path.display()
        )))
    }

    pub(crate) fn from_stored(value: impl Into<String>) -> Result<Self, MemoryScopeError> {
        let value = value.into();
        let git_prefix = format!("v{MEMORY_PROJECT_KEY_VERSION}:git:");
        let path_prefix = format!("v{MEMORY_PROJECT_KEY_VERSION}:path:");
        let derived = if let Some(origin) = value.strip_prefix(&git_prefix) {
            Self::from_git_origin(origin)
        } else if let Some(path) = value.strip_prefix(&path_prefix) {
            Self::from_canonical_path(path)
        } else {
            return Err(MemoryScopeError::InvalidProjectKey);
        };
        match derived {
            Ok(derived) if derived.0 == value => Ok(derived),
            _ => Err(MemoryScopeError::InvalidProjectKey),
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn artifact_digest(&self) -> String {
        let digest = Sha256::digest(self.as_str().as_bytes());
        format!("{digest:x}")
    }
}

/// Traversal-safe path to a cited memory artifact, relative to its scoped root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryCitationPath(String);

impl MemoryCitationPath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, MemoryScopeError> {
        let path = path.as_ref();
        let Some(path_text) = path.to_str() else {
            return Err(MemoryScopeError::InvalidCitationPath);
        };
        let has_forbidden_component = path_text
            .split(['/', '\\'])
            .any(|component| component.is_empty() || matches!(component, "." | ".."));
        if path_text.is_empty()
            || path_text.chars().any(char::is_control)
            || has_forbidden_component
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(MemoryScopeError::InvalidCitationPath);
        }
        Ok(Self(path_text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn from_stored(path: String) -> Result<Self, MemoryScopeError> {
        Self::new(path)
    }
}

impl fmt::Display for MemoryCitationPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for MemoryProjectKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Immutable memory binding recorded for one source thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryScope {
    pub thread_id: ThreadId,
    pub clanker_id: Option<CanonicalClankerId>,
    pub project_key: MemoryProjectKey,
    pub parent_thread_id: Option<ThreadId>,
    pub recorded_at: DateTime<Utc>,
}

impl MemoryScope {
    pub fn selection_scope(&self) -> MemorySelectionScope {
        match self.clanker_id.as_ref() {
            Some(clanker_id) => MemorySelectionScope::Named {
                clanker_id: clanker_id.clone(),
                project_key: self.project_key.clone(),
            },
            None => MemorySelectionScope::Anonymous,
        }
    }
}

/// Visibility policy persisted with every generated memory unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryVisibility {
    PrivateCharacter,
    ProjectShared,
    GlobalUserPreference,
    AnonymousLegacy,
}

impl MemoryVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateCharacter => "private_character",
            Self::ProjectShared => "project_shared",
            Self::GlobalUserPreference => "global_user_preference",
            Self::AnonymousLegacy => "anonymous_legacy",
        }
    }
}

impl FromStr for MemoryVisibility {
    type Err = MemoryScopeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "private_character" => Ok(Self::PrivateCharacter),
            "project_shared" => Ok(Self::ProjectShared),
            "global_user_preference" => Ok(Self::GlobalUserPreference),
            "anonymous_legacy" => Ok(Self::AnonymousLegacy),
            _ => Err(MemoryScopeError::InvalidVisibility {
                visibility: value.to_string(),
            }),
        }
    }
}

/// Scope used by deterministic startup memory selection.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemorySelectionScope {
    Named {
        clanker_id: CanonicalClankerId,
        project_key: MemoryProjectKey,
    },
    Anonymous,
}

impl MemorySelectionScope {
    pub fn phase2_key(&self) -> String {
        match self {
            Self::Named {
                clanker_id,
                project_key,
            } => format!(
                "character:{}:project:{}",
                clanker_id.as_str(),
                project_key.artifact_digest()
            ),
            Self::Anonymous => "anonymous".to_string(),
        }
    }
}

/// DB-backed memory record selected for one startup context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedMemoryRecord {
    pub output: Stage1Output,
    pub clanker_id: Option<CanonicalClankerId>,
    pub project_key: Option<MemoryProjectKey>,
    pub visibility: MemoryVisibility,
    pub parent_thread_id: Option<ThreadId>,
    pub citation_path: Option<MemoryCitationPath>,
    pub usage_count: u64,
    pub last_usage: Option<DateTime<Utc>>,
}

/// Borrowed stage-1 model output committed with immutable memory provenance.
#[derive(Debug, Clone, Copy)]
pub struct Stage1MemoryPayload<'a> {
    pub source_updated_at: i64,
    pub raw_memory: &'a str,
    pub rollout_summary: &'a str,
    pub rollout_slug: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScopeRegistration {
    Inserted,
    ReplayedExact,
}

/// Store-level reset result. Filesystem cleanup consumes these affected scopes later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryResetReceipt {
    pub removed_outputs: u64,
    pub affected_scopes: Vec<MemorySelectionScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRekeyReceipt {
    pub updated_scopes: u64,
    pub updated_outputs: u64,
    pub affected_scopes: Vec<MemorySelectionScope>,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryScopeError {
    #[error("invalid canonical clanker id {id:?}")]
    InvalidCanonicalClankerId { id: String },
    #[error("character id {id:?} is not an exact canonical catalog match")]
    UnverifiedCanonicalClankerId { id: String },
    #[error("invalid git origin for project identity")]
    InvalidProjectOrigin,
    #[error("project path must be normalized absolute UTF-8 without control characters")]
    InvalidProjectPath,
    #[error("citation path must be normalized relative UTF-8 without traversal or controls")]
    InvalidCitationPath,
    #[error("invalid versioned memory project key")]
    InvalidProjectKey,
    #[error("invalid memory visibility {visibility:?}")]
    InvalidVisibility { visibility: String },
    #[error("memory scope for thread {thread_id} conflicts with its immutable binding")]
    ScopeConflict { thread_id: ThreadId },
    #[error("memory scope for thread {thread_id} was not found")]
    ScopeNotFound { thread_id: ThreadId },
    #[error("memory scope for thread {thread_id} is anonymous")]
    AnonymousScope { thread_id: ThreadId },
    #[error("memory output for thread {thread_id} conflicts with its registered scope")]
    OutputScopeConflict { thread_id: ThreadId },
    #[error("named memory output for thread {thread_id} has no citation")]
    MissingCitation { thread_id: ThreadId },
    #[error("{visibility:?} memory output for thread {thread_id} lacks source provenance")]
    MissingSourceProvenance {
        thread_id: ThreadId,
        visibility: MemoryVisibility,
    },
    #[error("cannot rekey character memory to {clanker_id}; target records already exist")]
    CharacterRekeyConflict { clanker_id: CanonicalClankerId },
    #[error("invalid stored thread id {value:?}")]
    InvalidStoredThreadId { value: String },
    #[error("invalid stored memory timestamp {value}")]
    InvalidStoredTimestamp { value: i64 },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Stored stage-1 memory extraction output for a single thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1Output {
    pub thread_id: ThreadId,
    pub rollout_path: PathBuf,
    pub source_updated_at: DateTime<Utc>,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub rollout_slug: Option<String>,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub generated_at: DateTime<Utc>,
}

/// Result of trying to claim a stage-1 memory extraction job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage1JobClaimOutcome {
    /// The caller owns the job and should continue with extraction.
    Claimed { ownership_token: String },
    /// Existing output is already newer than or equal to the source rollout.
    SkippedUpToDate,
    /// Another worker currently owns a fresh lease for this job.
    SkippedRunning,
    /// The job is in backoff and should not be retried yet.
    SkippedRetryBackoff,
    /// The job has exhausted retries and should not be retried automatically.
    SkippedRetryExhausted,
}

/// Claimed stage-1 job with thread metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage1JobClaim {
    pub thread: ThreadMetadata,
    pub ownership_token: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Stage1StartupClaimParams<'a> {
    pub scan_limit: usize,
    pub max_claimed: usize,
    pub max_age_days: i64,
    pub min_rollout_idle_hours: i64,
    pub allowed_sources: &'a [String],
    pub lease_seconds: i64,
}

/// Result of trying to claim a phase-2 consolidation job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase2JobClaimOutcome {
    /// The caller owns the global lock and may inspect the memory workspace.
    Claimed {
        ownership_token: String,
        /// Snapshot of `input_watermark` at claim time.
        input_watermark: i64,
    },
    /// The global job is in retry backoff.
    SkippedRetryUnavailable,
    /// The global job completed recently enough that consolidation is cooling down.
    SkippedCooldown,
    /// Another worker currently owns a fresh global consolidation lease.
    SkippedRunning,
}
