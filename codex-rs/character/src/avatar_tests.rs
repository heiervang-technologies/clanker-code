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

fn write_pack(
    root: &Path,
    manifest: serde_json::Value,
    width: u32,
    height: u32,
    extension: &str,
) -> AvatarSelector {
    let pack = root.join("avatar");
    fs::create_dir_all(&pack).expect("create avatar pack");
    fs::write(
        pack.join("avatar.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize avatar manifest"),
    )
    .expect("write avatar manifest");
    image::RgbImage::from_pixel(width, height, image::Rgb([0, 0, 0]))
        .save(pack.join(format!("spritesheet.{extension}")))
        .expect("write spritesheet");
    AvatarSelector("avatar/avatar.json".to_string())
}

fn ansi_manifest(extension: &str, columns: u32) -> serde_json::Value {
    json!({
        "renderMode": "ansi-half-block",
        "spritesheetPath": format!("spritesheet.{extension}"),
        "frame": {"width": 24, "height": 24, "columns": columns, "rows": 1},
        "animations": {"idle": {"frames": [0], "fps": 8}}
    })
}

#[test]
fn accepts_terminal_default_and_returns_normalized_defaults() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        json!({"spritesheetPath": "spritesheet.png"}),
        /*width*/ 1536,
        /*height*/ 1872,
        "png",
    );

    let pack = validate_avatar_selector(temp.path(), &selector).expect("valid terminal pack");

    assert_eq!(pack.frame, AvatarFrameSpec::terminal_default());
    assert_eq!(pack.frame_count, 72);
    assert_eq!(pack.animations.len(), 14);
    let mut animation_names = pack
        .animations
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    animation_names.sort_unstable();
    assert_eq!(
        animation_names,
        vec![
            "bounce",
            "failed",
            "idle",
            "jumping",
            "move_left",
            "move_right",
            "review",
            "running",
            "running-left",
            "running-right",
            "sad",
            "waiting",
            "wave",
            "waving",
        ]
    );
    assert_eq!(
        pack.animations["idle"].frames[0].duration,
        Duration::from_millis(1680)
    );
    assert_eq!(pack.animations["running-right"].frames[0].sprite_index, 8);
    assert_eq!(pack.animations["running-right"].loop_start, Some(24));
    assert_eq!(pack.animations["sad"].fallback, "idle");
}

#[test]
fn accepts_ansi_multi_frame_grid_and_normalizes_override() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        ansi_manifest("png", /*columns*/ 2),
        /*width*/ 48,
        /*height*/ 24,
        "png",
    );

    let pack = validate_avatar_selector(temp.path(), &selector).expect("valid ANSI pack");

    assert_eq!(pack.frame_count, 2);
    assert_eq!(
        pack.animations["idle"].frames[0].duration,
        Duration::from_millis(125)
    );
    assert_eq!(pack.animations["idle"].loop_start, Some(0));
}

#[test]
fn rejects_swapped_terminal_and_ansi_geometry_assumptions() {
    let terminal = TempDir::new().expect("temp dir");
    let terminal_selector = write_pack(
        terminal.path(),
        json!({
            "spritesheetPath": "spritesheet.png",
            "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1}
        }),
        /*width*/ 24,
        /*height*/ 24,
        "png",
    );
    let ansi = TempDir::new().expect("temp dir");
    let ansi_selector = write_pack(
        ansi.path(),
        json!({
            "renderMode": "ansi-half-block",
            "spritesheetPath": "spritesheet.png",
            "frame": {"width": 24, "height": 48, "columns": 1, "rows": 1}
        }),
        /*width*/ 24,
        /*height*/ 48,
        "png",
    );

    assert_eq!(
        validate_avatar_selector(terminal.path(), &terminal_selector)
            .expect_err("reject 24x24 terminal")
            .kind,
        AvatarPackErrorKind::InvalidGeometry
    );
    assert_eq!(
        validate_avatar_selector(ansi.path(), &ansi_selector)
            .expect_err("reject 24x48 ANSI")
            .kind,
        AvatarPackErrorKind::InvalidGeometry
    );
}

#[test]
fn terminal_explicit_grid_still_validates_retained_defaults() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        json!({
            "spritesheetPath": "spritesheet.png",
            "frame": {"width": 1536, "height": 1872, "columns": 1, "rows": 1}
        }),
        /*width*/ 1536,
        /*height*/ 1872,
        "png",
    );

    assert_eq!(
        validate_avatar_selector(temp.path(), &selector)
            .expect_err("retained defaults reference later frames")
            .kind,
        AvatarPackErrorKind::InvalidAnimation
    );
}

#[test]
fn terminal_override_can_fall_back_to_retained_sad_track() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        json!({
            "spritesheetPath": "spritesheet.png",
            "animations": {
                "wave": {"frames": [0], "fps": 10, "loop": false, "fallback": "sad"}
            }
        }),
        /*width*/ 1536,
        /*height*/ 1872,
        "png",
    );

    let pack = validate_avatar_selector(temp.path(), &selector).expect("valid fallback");
    assert_eq!(pack.animations["wave"].fallback, "sad");
    assert_eq!(pack.animations["wave"].loop_start, None);
    assert!(pack.animations.contains_key("running-left"));
}

#[test]
fn rejects_bad_override_fps_index_and_fallback() {
    for (name, animation) in [
        ("fps", json!({"frames": [0], "fps": 0})),
        ("index", json!({"frames": [2], "fps": 8})),
        (
            "fallback",
            json!({"frames": [0], "fps": 8, "fallback": "missing"}),
        ),
    ] {
        let temp = TempDir::new().expect("temp dir");
        let mut manifest = ansi_manifest("png", /*columns*/ 2);
        manifest["animations"][name] = animation;
        let selector = write_pack(
            temp.path(),
            manifest,
            /*width*/ 48,
            /*height*/ 24,
            "png",
        );
        assert_eq!(
            validate_avatar_selector(temp.path(), &selector)
                .expect_err("reject malformed override")
                .kind,
            AvatarPackErrorKind::InvalidAnimation
        );
    }
}

#[test]
fn rejects_missing_malformed_and_traversing_pack_files() {
    let temp = TempDir::new().expect("temp dir");
    let missing = AvatarSelector("avatar/avatar.json".to_string());
    assert_eq!(
        validate_avatar_selector(temp.path(), &missing)
            .expect_err("missing manifest")
            .kind,
        AvatarPackErrorKind::MissingManifest
    );

    let pack = temp.path().join("avatar");
    fs::create_dir_all(&pack).expect("create pack");
    fs::write(pack.join("avatar.json"), "{").expect("write malformed manifest");
    assert_eq!(
        validate_avatar_selector(temp.path(), &missing)
            .expect_err("malformed manifest")
            .kind,
        AvatarPackErrorKind::InvalidManifest
    );

    fs::write(
        pack.join("avatar.json"),
        r#"{"renderMode":"ansi-half-block","spritesheetPath":"../outside.png"}"#,
    )
    .expect("write traversing manifest");
    assert_eq!(
        validate_avatar_selector(temp.path(), &missing)
            .expect_err("traversing spritesheet")
            .kind,
        AvatarPackErrorKind::InvalidSpritesheet
    );
}

#[test]
fn rejects_corrupt_image_payload_after_accepting_its_header_dimensions() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        ansi_manifest("png", /*columns*/ 1),
        /*width*/ 24,
        /*height*/ 24,
        "png",
    );
    let spritesheet = temp.path().join("avatar/spritesheet.png");
    fs::write(&spritesheet, CORRUPT_IDAT_PNG)
        .expect("write PNG with valid header but corrupt payload");
    assert_eq!(
        image::image_dimensions(&spritesheet).expect("header dimensions remain readable"),
        (24, 24)
    );
    image::open(&spritesheet).expect_err("pixel decode rejects corrupt IDAT payload");

    assert_eq!(
        validate_avatar_selector(temp.path(), &selector)
            .expect_err("full image decode must fail")
            .kind,
        AvatarPackErrorKind::InvalidSpritesheet
    );
}

#[test]
fn reports_multiple_invalid_animation_tracks_in_name_order() {
    let temp = TempDir::new().expect("temp dir");
    let selector = write_pack(
        temp.path(),
        json!({
            "renderMode": "ansi-half-block",
            "spritesheetPath": "spritesheet.png",
            "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1},
            "animations": {
                "zulu": {"frames": [7]},
                "alpha": {"frames": [3]}
            }
        }),
        /*width*/ 24,
        /*height*/ 24,
        "png",
    );

    for _ in 0..16 {
        let error = validate_avatar_selector(temp.path(), &selector)
            .expect_err("both override tracks are invalid");
        assert!(error.message.contains("animation alpha"), "{error}");
    }
}

#[test]
fn decodes_locked_fixture_and_production_codecs() {
    for extension in ["png", "jpg", "gif", "webp"] {
        let temp = TempDir::new().expect("temp dir");
        let selector = write_pack(
            temp.path(),
            ansi_manifest(extension, /*columns*/ 1),
            /*width*/ 24,
            /*height*/ 24,
            extension,
        );
        validate_avatar_selector(temp.path(), &selector).expect("codec pack is valid");
    }

    let temp = TempDir::new().expect("temp dir");
    let pack = temp.path().join("avatar");
    fs::create_dir_all(&pack).expect("create avatar pack");
    fs::write(
        pack.join("avatar.json"),
        serde_json::to_vec_pretty(&ansi_manifest("ppm", /*columns*/ 1))
            .expect("serialize manifest"),
    )
    .expect("write manifest");
    fs::write(
        pack.join("spritesheet.ppm"),
        format!("P3\n24 24\n255\n{}\n", "0 0 0 ".repeat(24 * 24)),
    )
    .expect("write P3 PPM");
    validate_avatar_selector(
        temp.path(),
        &AvatarSelector("avatar/avatar.json".to_string()),
    )
    .expect("P3 PPM is valid");
}
