// Modified by Heiervang Technologies.
use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

use super::*;

const CORRUPT_IDAT_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 24, 0, 0, 0, 24, 8, 2,
    0, 0, 0, 111, 21, 170, 175, 0, 0, 0, 17, 73, 68, 65, 84, 110, 111, 116, 45, 97, 45, 122, 108,
    105, 98, 45, 115, 116, 114, 101, 97, 109, 13, 130, 204, 36, 0, 0, 0, 0, 73, 69, 78, 68, 174,
    66, 96, 130,
];

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

fn write_ansi_pack(package: &Path, selector: &str) {
    let manifest_path = package.join(selector);
    let pack = manifest_path.parent().expect("avatar pack directory");
    fs::create_dir_all(pack).expect("create avatar pack");
    fs::write(
        &manifest_path,
        r#"{
            "renderMode": "ansi-half-block",
            "spritesheetPath": "spritesheet.png",
            "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1}
        }"#,
    )
    .expect("write avatar manifest");
    image::RgbaImage::from_pixel(24, 24, image::Rgba([0, 0, 0, 0]))
        .save(pack.join("spritesheet.png"))
        .expect("write avatar spritesheet");
}

fn write_character(root: &Path, directory: &str, value: serde_json::Value) -> PathBuf {
    let path = write_manifest(root, directory, value.clone());
    let package = path.parent().expect("character package");
    if let Some(selector) = value.get("avatar").and_then(serde_json::Value::as_str)
        && !selector.contains("..")
    {
        write_ansi_pack(package, selector);
    }
    if let Some(overrides) = value
        .get("avatarByMode")
        .and_then(serde_json::Value::as_object)
    {
        for selector in overrides.values().filter_map(serde_json::Value::as_str) {
            if !selector.contains("..") {
                write_ansi_pack(package, selector);
            }
        }
    }
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
    let path = write_character(temp.path(), "chloe", manifest("chloe", "Chloe", &["chlo"]));

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
fn validates_every_default_and_mode_avatar_pack() {
    let cases = [
        (
            "missing-default",
            "avatar",
            ValidationIssueCode::MissingAvatar,
        ),
        (
            "missing-override",
            "avatarByMode.locked_in",
            ValidationIssueCode::InvalidManifest,
        ),
        ("malformed", "avatar", ValidationIssueCode::InvalidManifest),
        (
            "missing-sheet",
            "avatar",
            ValidationIssueCode::InvalidManifest,
        ),
        (
            "traversing-sheet",
            "avatar",
            ValidationIssueCode::InvalidManifest,
        ),
        (
            "bad-geometry",
            "avatar",
            ValidationIssueCode::InvalidManifest,
        ),
        (
            "bad-animation",
            "avatar",
            ValidationIssueCode::InvalidManifest,
        ),
        (
            "corrupt-image",
            "avatar",
            ValidationIssueCode::InvalidManifest,
        ),
    ];
    for (case, expected_path, expected_code) in cases {
        let temp = TempDir::new().expect("temp dir");
        let value = manifest(case, case, &[]);
        let path = write_character(temp.path(), case, value);
        let package = path.parent().expect("package");
        match case {
            "missing-default" => fs::remove_file(package.join("avatar/avatar.json")).unwrap(),
            "missing-override" => {
                fs::remove_file(package.join("avatar/locked-in/avatar.json")).unwrap()
            }
            "malformed" => fs::write(package.join("avatar/avatar.json"), "{").unwrap(),
            "missing-sheet" => fs::remove_file(package.join("avatar/spritesheet.png")).unwrap(),
            "traversing-sheet" => fs::write(
                package.join("avatar/avatar.json"),
                r#"{"renderMode":"ansi-half-block","spritesheetPath":"../sheet.png"}"#,
            )
            .unwrap(),
            "bad-geometry" => fs::write(
                package.join("avatar/avatar.json"),
                r#"{"renderMode":"ansi-half-block","spritesheetPath":"spritesheet.png","frame":{"width":24,"height":48,"columns":1,"rows":1}}"#,
            )
            .unwrap(),
            "bad-animation" => fs::write(
                package.join("avatar/avatar.json"),
                r#"{"renderMode":"ansi-half-block","spritesheetPath":"spritesheet.png","frame":{"width":24,"height":24,"columns":1,"rows":1},"animations":{"idle":{"frames":[2]}}}"#,
            )
            .unwrap(),
            "corrupt-image" => {
                fs::write(package.join("avatar/spritesheet.png"), CORRUPT_IDAT_PNG).unwrap()
            }
            _ => unreachable!(),
        }

        let single = validate_manifest_path(&path);
        let all = CharacterCatalog::load(temp.path());
        assert!(
            single.errors.iter().any(|error| {
                error.code == expected_code && error.path.as_deref() == Some(expected_path)
            }),
            "single validation case {case}: {:?}",
            single.errors
        );
        assert!(
            all.errors().iter().any(|error| {
                error.code == expected_code && error.path.as_deref() == Some(expected_path)
            }),
            "catalog validation case {case}: {:?}",
            all.errors()
        );
    }
}

#[test]
fn resolves_exact_then_casefold_then_alias_with_unrelated_invalid_packages() {
    let temp = TempDir::new().expect("temp dir");
    write_character(
        temp.path(),
        "c3ph0",
        manifest("c3ph0", "C3PH0", &["c3p-h0", "cepho"]),
    );
    write_manifest(temp.path(), "broken", json!({"not": "a character"}));
    write_manifest(temp.path(), "malformed", json!({"schemaVersion": 1}));
    write_manifest(
        temp.path(),
        "missing-avatar",
        manifest("missing-avatar", "Missing Avatar", &[]),
    );
    let catalog = CharacterCatalog::load(temp.path());

    assert_eq!(
        catalog.resolve("c3ph0").unwrap().match_kind,
        MatchKind::ExactCanonical
    );
    assert_eq!(
        catalog.resolve("C3PH0").unwrap().match_kind,
        MatchKind::CasefoldCanonical
    );
    assert_eq!(
        catalog.resolve("C3P-H0").unwrap().match_kind,
        MatchKind::ExplicitAlias
    );
    for input in ["missing-avatar", "MISSING-AVATAR"] {
        let errors = catalog
            .resolve(input)
            .expect_err("requested missing avatar is invalid");
        assert!(errors.iter().any(|error| {
            error.code == ValidationIssueCode::MissingAvatar
                && error.path.as_deref() == Some("avatar")
        }));
    }
}

#[test]
fn requested_invalid_package_returns_candidate_errors_for_every_recoverable_key() {
    let temp = TempDir::new().expect("temp dir");
    let mismatched = manifest("declared", "Declared", &["alias"]);
    write_character(temp.path(), "directory", mismatched);
    let catalog = CharacterCatalog::load(temp.path());

    for input in ["directory", "DIRECTORY", "declared", "DECLARED", "alias"] {
        let errors = catalog
            .resolve(input)
            .expect_err("mismatched candidate is invalid");
        assert!(
            errors.iter().any(|error| {
                error.code == ValidationIssueCode::InvalidManifest
                    && error.path.as_deref() == Some("id")
            }),
            "input {input}: {errors:?}"
        );
    }
}

#[test]
fn malformed_requested_directory_returns_its_validation_error() {
    let temp = TempDir::new().expect("temp dir");
    let path = write_manifest(temp.path(), "broken", json!({"schemaVersion": 1}));
    fs::write(path, "{").expect("write malformed manifest");
    let catalog = CharacterCatalog::load(temp.path());

    for input in ["broken", "BROKEN"] {
        let errors = catalog
            .resolve(input)
            .expect_err("broken candidate is invalid");
        assert_eq!(errors[0].code, ValidationIssueCode::InvalidManifest);
    }
}

#[test]
fn classifies_alias_collisions_in_stable_order() {
    let temp = TempDir::new().expect("temp dir");
    write_character(
        temp.path(),
        "c3ph0",
        manifest("c3ph0", "C3PH0", &["shared-alias"]),
    );
    write_character(
        temp.path(),
        "imposter",
        manifest("imposter", "Imposter", &["shared-alias", "c3ph0"]),
    );
    let catalog = CharacterCatalog::load(temp.path());

    assert_eq!(
        catalog
            .errors()
            .iter()
            .filter_map(|error| error.conflict_kind.map(|kind| (error.code, kind)))
            .collect::<Vec<_>>(),
        vec![
            (
                ValidationIssueCode::AliasCollision,
                ConflictKind::AliasVsAlias
            ),
            (
                ValidationIssueCode::AliasCollision,
                ConflictKind::AliasVsCanonical
            ),
        ]
    );
}

#[test]
fn duplicate_declared_ids_preserve_canonical_collision_despite_directory_mismatch() {
    let temp = TempDir::new().expect("temp dir");
    write_character(temp.path(), "first", manifest("duplicate", "First", &[]));
    write_character(temp.path(), "second", manifest("duplicate", "Second", &[]));
    let catalog = CharacterCatalog::load(temp.path());

    assert_eq!(
        catalog
            .errors()
            .iter()
            .map(|error| error.code)
            .collect::<Vec<_>>(),
        vec![
            ValidationIssueCode::InvalidManifest,
            ValidationIssueCode::InvalidManifest,
            ValidationIssueCode::CanonicalCollision,
        ]
    );
    assert_eq!(
        catalog
            .errors()
            .last()
            .and_then(|error| error.conflict_kind),
        Some(ConflictKind::CanonicalVsCanonical)
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
