mod manifest;

pub use manifest::AvatarPlacement;
pub use manifest::AvatarSelector;
pub use manifest::CharacterCatalog;
pub use manifest::CharacterManifestV1;
pub use manifest::ConflictKind;
pub use manifest::MatchKind;
pub use manifest::ResolvedCharacter;
pub use manifest::ValidationIssue;
pub use manifest::ValidationIssueCode;
pub use manifest::ValidationReport;
pub use manifest::validate_manifest_path;

pub const CHARACTER_SCHEMA_VERSION: u32 = 1;
