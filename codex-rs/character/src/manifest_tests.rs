use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

use super::*;

fn write_manifest(root: &Path, directory: &str, value: serde_json::Value) -> PathBuf {
    let package = root.join("characters").join(directory);
    fs::create_dir_all(&package).expect("create character package");
    let path = package.join("character.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("serialize manifest"),
    )
    .expect("write character manifest");
    path
}

fn manifest(id: &str, display_name: &str, aliases: &[&str]) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "id": id,
        "displayName": display_name,
        "aliases": aliases,
        "avatar": "avatar/avatar.json",
        "avatarByMode": {"locked_in": "avatar/locked-in/avatar.json"},
        "avatarPlacement": "below_left"
    })
}

#[test]
fn validates_manifest_and_typed_avatar_contract() {
    let temp = TempDir::new().expect("temp dir");
    let path = write_manifest(temp.path(), "chloe", manifest("chloe", "Chloe", &["chlo"]));

    let report = validate_manifest_path(&path);

    assert_eq!(report.errors, Vec::new());
    let parsed = report.manifest.expect("valid manifest");
    assert_eq!(parsed.id, "chloe");
    assert_eq!(parsed.avatar_placement, AvatarPlacement::BelowLeft);
    assert_eq!(
        parsed.avatar_by_mode[&ModeKind::LockedIn].as_str(),
        "avatar/locked-in/avatar.json"
    );
}

#[test]
fn missing_avatar_has_stable_error_code() {
    let temp = TempDir::new().expect("temp dir");
    let path = write_manifest(
        temp.path(),
        "avatarless",
        json!({
            "schemaVersion": 1,
            "id": "avatarless",
            "displayName": "Avatarless",
            "aliases": []
        }),
    );

    let report = validate_manifest_path(&path);

    assert_eq!(
        report.errors,
        vec![
            ValidationIssue::new(ValidationIssueCode::MissingAvatar, "avatar is required")
                .at_path("avatar")
        ]
    );
}

#[test]
fn resolves_exact_then_casefold_then_alias() {
    let temp = TempDir::new().expect("temp dir");
    write_manifest(
        temp.path(),
        "c3ph0",
        manifest("c3ph0", "C3PH0", &["c3p-h0", "cepho"]),
    );
    let catalog = CharacterCatalog::load(temp.path());

    let exact = catalog.resolve("c3ph0").expect("resolve exact");
    let casefold = catalog.resolve("C3PH0").expect("resolve casefold");
    let alias = catalog.resolve("C3P-H0").expect("resolve alias");

    assert_eq!(exact.match_kind, MatchKind::ExactCanonical);
    assert_eq!(casefold.match_kind, MatchKind::CasefoldCanonical);
    assert_eq!(alias.match_kind, MatchKind::ExplicitAlias);
    assert_eq!(alias.manifest.id, "c3ph0");
}

#[test]
fn classifies_alias_collisions_in_stable_order() {
    let temp = TempDir::new().expect("temp dir");
    write_manifest(
        temp.path(),
        "c3ph0",
        manifest("c3ph0", "C3PH0", &["shared-alias"]),
    );
    write_manifest(
        temp.path(),
        "imposter",
        manifest("imposter", "Imposter", &["shared-alias", "c3ph0"]),
    );

    let catalog = CharacterCatalog::load(temp.path());

    assert_eq!(
        catalog
            .errors()
            .iter()
            .map(|error| (error.code, error.conflict_kind))
            .collect::<Vec<_>>(),
        vec![
            (
                ValidationIssueCode::AliasCollision,
                Some(ConflictKind::AliasVsAlias)
            ),
            (
                ValidationIssueCode::AliasCollision,
                Some(ConflictKind::AliasVsCanonical)
            )
        ]
    );
}

#[test]
fn canonical_collision_is_reserved_for_two_canonical_records() {
    let temp = TempDir::new().expect("temp dir");
    write_manifest(temp.path(), "first", manifest("duplicate", "First", &[]));
    write_manifest(temp.path(), "second", manifest("duplicate", "Second", &[]));

    let catalog = CharacterCatalog::load(temp.path());

    assert_eq!(
        catalog
            .errors()
            .iter()
            .map(|error| (error.code, error.conflict_kind))
            .collect::<Vec<_>>(),
        vec![(
            ValidationIssueCode::CanonicalCollision,
            Some(ConflictKind::CanonicalVsCanonical)
        )]
    );
}

#[test]
fn rejects_traversing_avatar_selectors() {
    let temp = TempDir::new().expect("temp dir");
    let path = write_manifest(
        temp.path(),
        "escape",
        json!({
            "schemaVersion": 1,
            "id": "escape",
            "displayName": "Escape",
            "avatar": "../avatar.json"
        }),
    );

    let report = validate_manifest_path(&path);

    assert_eq!(report.errors[0].code, ValidationIssueCode::InvalidManifest);
    assert_eq!(report.errors[0].path.as_deref(), Some("avatar"));
}
