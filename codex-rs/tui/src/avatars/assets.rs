//! Bundled offline avatar packs.

use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use codex_character::AvatarSelector;
use codex_character::validate_avatar_selector;

const CLANKER_MANIFEST: &str = include_str!("../../assets/clanker/avatar.json");
const CLANKER_SHEET: &[u8] = include_bytes!("../../assets/clanker/sheet.png");
const CHLOE_MANIFEST: &str = include_str!("../../assets/chloe-r2-09/avatar.json");
const CHLOE_SHEET: &[u8] = include_bytes!("../../assets/chloe-r2-09/sheet.png");
const CHLOE_LOCKED_IN_MANIFEST: &str =
    include_str!("../../assets/chloe-r2-09-locked-in/avatar.json");
const CHLOE_LOCKED_IN_SHEET: &[u8] = include_bytes!("../../assets/chloe-r2-09-locked-in/sheet.png");
const CLANKER_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "clanker",
  "displayName": "Rusty Clanker",
  "aliases": ["rusty"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CHLOE_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "chloe",
  "displayName": "Chloe",
  "aliases": ["cleo"],
  "avatar": "avatar/default/avatar.json",
  "avatarByMode": {
    "locked_in": "avatar/locked-in/avatar.json"
  },
  "avatarPlacement": "far_right",
  "voiceProfile": "cleo"
}"#;
const C3PH0_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "c3ph0",
  "displayName": "C-3PH0",
  "aliases": ["c-3ph0"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const HAI_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "hai",
  "displayName": "HAI",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CLAUTIST_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "clautist",
  "displayName": "Clautist",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const JASONELLE_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "jasonelle",
  "displayName": "Jasonelle",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CENTURION_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "centurion",
  "displayName": "Centurion",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CRYSTAL_MOM_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "crystal-mom",
  "displayName": "Crystal-Mom",
  "aliases": ["crystal_mom"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const SNOOP_KUBE_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "snoop-kube",
  "displayName": "Snoop-Kube",
  "aliases": ["snoop-kubernetes", "snoop_kubernetes", "snoop_kube"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const PENNY_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "penny",
  "displayName": "Penny",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const LAWRENCE_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "lawrence",
  "displayName": "Lawrence",
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CLAUDIAN_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "claudian",
  "displayName": "Claudian",
  "aliases": ["claudians"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const CHATGEPERT_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "chatgepert",
  "displayName": "Chatgepert",
  "aliases": ["chatgeperts", "gepert", "geperts"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const GEMINOID_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "geminoid",
  "displayName": "Geminoid",
  "aliases": ["geminoids"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const XINAN_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "xinan",
  "displayName": "Xinan",
  "aliases": ["xinans"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;
const GONKER_CHARACTER_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "id": "gonker",
  "displayName": "Gonker",
  "aliases": ["gonkers"],
  "avatar": "avatar/default/avatar.json",
  "avatarPlacement": "far_right"
}"#;

#[derive(Clone, Copy)]
struct BundledAvatar {
    id: &'static str,
    manifest: &'static str,
    sheet: &'static [u8],
}

struct BundledCharacter {
    id: &'static str,
    aliases: &'static [&'static str],
    manifest: &'static str,
    avatars: &'static [BundledAvatar],
    install_eagerly: bool,
}

macro_rules! bundled_avatar {
    ($id:literal) => {
        BundledAvatar {
            id: $id,
            manifest: include_str!(concat!("../../assets/", $id, "/avatar.json")),
            sheet: include_bytes!(concat!("../../assets/", $id, "/sheet.png")),
        }
    };
}

macro_rules! lazy_character {
    ($id:literal, $manifest:ident, [$($alias:literal),* $(,)?]) => {
        BundledCharacter {
            id: $id,
            aliases: &[$($alias),*],
            manifest: $manifest,
            avatars: &[BundledAvatar {
                id: "default",
                manifest: include_str!(concat!("../../assets/", $id, "/avatar.json")),
                sheet: include_bytes!(concat!("../../assets/", $id, "/sheet.png")),
            }],
            install_eagerly: false,
        }
    };
}

const BUNDLED_AVATARS: &[BundledAvatar] = &[
    bundled_avatar!("clanker"),
    bundled_avatar!("chloe-r2-09"),
    bundled_avatar!("chloe-r2-09-locked-in"),
    bundled_avatar!("c3ph0"),
    bundled_avatar!("hai"),
    bundled_avatar!("clautist"),
    bundled_avatar!("jasonelle"),
    bundled_avatar!("centurion"),
    bundled_avatar!("crystal-mom"),
    bundled_avatar!("snoop-kube"),
    bundled_avatar!("penny"),
    bundled_avatar!("lawrence"),
    bundled_avatar!("claudian"),
    bundled_avatar!("chatgepert"),
    bundled_avatar!("geminoid"),
    bundled_avatar!("xinan"),
    bundled_avatar!("gonker"),
];

const BUNDLED_CHARACTERS: &[BundledCharacter] = &[
    BundledCharacter {
        id: "clanker",
        aliases: &["rusty"],
        manifest: CLANKER_CHARACTER_MANIFEST,
        avatars: &[BundledAvatar {
            id: "default",
            manifest: CLANKER_MANIFEST,
            sheet: CLANKER_SHEET,
        }],
        install_eagerly: true,
    },
    BundledCharacter {
        id: "chloe",
        aliases: &["cleo"],
        manifest: CHLOE_CHARACTER_MANIFEST,
        avatars: &[
            BundledAvatar {
                id: "default",
                manifest: CHLOE_MANIFEST,
                sheet: CHLOE_SHEET,
            },
            BundledAvatar {
                id: "locked-in",
                manifest: CHLOE_LOCKED_IN_MANIFEST,
                sheet: CHLOE_LOCKED_IN_SHEET,
            },
        ],
        install_eagerly: true,
    },
    lazy_character!("c3ph0", C3PH0_CHARACTER_MANIFEST, ["c-3ph0"]),
    lazy_character!("hai", HAI_CHARACTER_MANIFEST, []),
    lazy_character!("clautist", CLAUTIST_CHARACTER_MANIFEST, []),
    lazy_character!("jasonelle", JASONELLE_CHARACTER_MANIFEST, []),
    lazy_character!("centurion", CENTURION_CHARACTER_MANIFEST, []),
    lazy_character!(
        "crystal-mom",
        CRYSTAL_MOM_CHARACTER_MANIFEST,
        ["crystal_mom"]
    ),
    lazy_character!(
        "snoop-kube",
        SNOOP_KUBE_CHARACTER_MANIFEST,
        ["snoop-kubernetes", "snoop_kubernetes", "snoop_kube"]
    ),
    lazy_character!("penny", PENNY_CHARACTER_MANIFEST, []),
    lazy_character!("lawrence", LAWRENCE_CHARACTER_MANIFEST, []),
    lazy_character!("claudian", CLAUDIAN_CHARACTER_MANIFEST, ["claudians"]),
    lazy_character!(
        "chatgepert",
        CHATGEPERT_CHARACTER_MANIFEST,
        ["chatgeperts", "gepert", "geperts"]
    ),
    lazy_character!("geminoid", GEMINOID_CHARACTER_MANIFEST, ["geminoids"]),
    lazy_character!("xinan", XINAN_CHARACTER_MANIFEST, ["xinans"]),
    lazy_character!("gonker", GONKER_CHARACTER_MANIFEST, ["gonkers"]),
];

/// Materialize built-in packs without overwriting an existing user-owned id.
pub(crate) fn ensure_bundled_avatars(codex_home: &Path) -> Result<()> {
    for avatar in BUNDLED_AVATARS {
        let avatar_dir = codex_home.join("avatars").join(avatar.id);
        let manifest = avatar_dir.join("avatar.json");
        if manifest.is_file() {
            validate_installed_avatar(&avatar_dir)?;
            continue;
        }
        fs::create_dir_all(&avatar_dir)
            .with_context(|| format!("create {}", avatar_dir.display()))?;
        fs::write(avatar_dir.join("sheet.png"), avatar.sheet)
            .with_context(|| format!("write bundled avatar in {}", avatar_dir.display()))?;
        fs::write(&manifest, avatar.manifest)
            .with_context(|| format!("write {}", manifest.display()))?;
        validate_installed_avatar(&avatar_dir)?;
    }
    for character in BUNDLED_CHARACTERS
        .iter()
        .filter(|character| character.install_eagerly)
    {
        ensure_bundled_character(codex_home, character)?;
    }
    Ok(())
}

pub fn ensure_bundled_character_for_name(codex_home: &Path, requested_name: &str) -> Result<()> {
    if let Some(character) = BUNDLED_CHARACTERS.iter().find(|character| {
        requested_name.eq_ignore_ascii_case(character.id)
            || character
                .aliases
                .iter()
                .any(|alias| requested_name.eq_ignore_ascii_case(alias))
    }) {
        ensure_bundled_character(codex_home, character)?;
    }
    Ok(())
}

fn ensure_bundled_character(codex_home: &Path, character: &BundledCharacter) -> Result<()> {
    let character_dir = codex_home.join("characters").join(character.id);
    let manifest_path = character_dir.join("character.json");
    if character_dir.exists() {
        return Ok(());
    }
    for avatar in character.avatars {
        let avatar_dir = character_dir.join("avatar").join(avatar.id);
        fs::create_dir_all(&avatar_dir)
            .with_context(|| format!("create {}", avatar_dir.display()))?;
        fs::write(avatar_dir.join("sheet.png"), avatar.sheet)
            .with_context(|| format!("write bundled avatar in {}", avatar_dir.display()))?;
        fs::write(avatar_dir.join("avatar.json"), avatar.manifest)
            .with_context(|| format!("write bundled avatar in {}", avatar_dir.display()))?;
        validate_installed_avatar(&avatar_dir)?;
    }
    fs::write(&manifest_path, character.manifest)
        .with_context(|| format!("write {}", manifest_path.display()))
}

fn validate_installed_avatar(avatar_dir: &Path) -> Result<()> {
    let manifest = avatar_dir.join("avatar.json");
    let package_root = avatar_dir
        .parent()
        .context("bundled avatar has no containing package directory")?;
    let directory_name = avatar_dir
        .file_name()
        .and_then(|name| name.to_str())
        .context("bundled avatar directory name is not UTF-8")?;
    let selector = AvatarSelector(format!("{directory_name}/avatar.json"));
    validate_avatar_selector(package_root, &selector)
        .map(drop)
        .with_context(|| format!("avatar pack is invalid: {}", manifest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_valid_avatar(dir: &Path, spritesheet_path: &str) {
        fs::create_dir_all(dir).unwrap();
        image::RgbaImage::new(24, 24)
            .save(dir.join(spritesheet_path))
            .unwrap();
        fs::write(
            dir.join("avatar.json"),
            format!(
                r#"{{
                    "renderMode":"ansi-half-block",
                    "spritesheetPath":"{spritesheet_path}",
                    "frame":{{"width":24,"height":24,"columns":1,"rows":1}},
                    "animations":{{"idle":{{"frames":[0],"fps":1}}}}
                }}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn bundled_avatars_install_without_overwriting_existing_pack() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("avatars/chloe-r2-09");
        write_valid_avatar(&existing, "user.png");
        let existing_manifest = fs::read_to_string(existing.join("avatar.json")).unwrap();

        ensure_bundled_avatars(dir.path()).unwrap();

        assert_eq!(
            fs::read_to_string(existing.join("avatar.json")).unwrap(),
            existing_manifest
        );
        assert!(
            dir.path()
                .join("avatars/chloe-r2-09-locked-in/avatar.json")
                .is_file()
        );
        let installed_avatar_ids = BUNDLED_AVATARS
            .iter()
            .filter(|avatar| {
                dir.path()
                    .join("avatars")
                    .join(avatar.id)
                    .join("avatar.json")
                    .is_file()
            })
            .map(|avatar| avatar.id)
            .collect::<Vec<_>>();
        assert_eq!(
            installed_avatar_ids,
            vec![
                "clanker",
                "chloe-r2-09",
                "chloe-r2-09-locked-in",
                "c3ph0",
                "hai",
                "clautist",
                "jasonelle",
                "centurion",
                "crystal-mom",
                "snoop-kube",
                "penny",
                "lawrence",
                "claudian",
                "chatgepert",
                "geminoid",
                "xinan",
                "gonker",
            ]
        );
        assert!(
            dir.path()
                .join("characters/chloe/avatar/locked-in/avatar.json")
                .is_file()
        );
        let installed_character_ids = BUNDLED_CHARACTERS
            .iter()
            .filter(|character| {
                dir.path()
                    .join("characters")
                    .join(character.id)
                    .join("character.json")
                    .is_file()
            })
            .map(|character| character.id)
            .collect::<Vec<_>>();
        assert_eq!(installed_character_ids, vec!["clanker", "chloe"]);
    }

    #[test]
    fn eager_install_preserves_custom_aliases_for_lazy_bundled_characters() {
        let dir = tempfile::tempdir().unwrap();
        let character_dir = dir.path().join("characters/orion");
        write_valid_avatar(&character_dir.join("avatar/default"), "sheet.png");
        fs::write(
            character_dir.join("character.json"),
            r#"{
                "schemaVersion":1,
                "id":"orion",
                "displayName":"Orion",
                "aliases":["hai"],
                "avatar":"avatar/default/avatar.json"
            }"#,
        )
        .unwrap();

        ensure_bundled_avatars(dir.path()).unwrap();

        let resolved = codex_character::CharacterCatalog::load(dir.path())
            .resolve("hai")
            .unwrap();
        assert_eq!(
            (resolved.manifest.id.as_str(), resolved.match_kind),
            ("orion", codex_character::MatchKind::ExplicitAlias)
        );
        assert!(!dir.path().join("characters/hai").exists());
    }

    #[test]
    fn partial_existing_avatar_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("avatars/chloe-r2-09");
        fs::create_dir_all(&existing).unwrap();
        fs::write(
            existing.join("avatar.json"),
            r#"{"spritesheetPath":"missing.png"}"#,
        )
        .unwrap();

        let error = ensure_bundled_avatars(dir.path()).unwrap_err();

        assert!(error.to_string().contains("avatar pack is invalid"));
    }

    #[test]
    fn traversal_spritesheet_path_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("avatars/chloe-r2-09");
        fs::create_dir_all(&existing).unwrap();
        image::RgbaImage::new(24, 24)
            .save(dir.path().join("outside.png"))
            .unwrap();
        fs::write(
            existing.join("avatar.json"),
            r#"{
                "renderMode":"ansi-half-block",
                "spritesheetPath":"../../outside.png",
                "frame":{"width":24,"height":24,"columns":1,"rows":1},
                "animations":{"idle":{"frames":[0],"fps":1}}
            }"#,
        )
        .unwrap();

        let error = ensure_bundled_avatars(dir.path()).unwrap_err();

        assert!(error.to_string().contains("avatar pack is invalid"));
        assert!(format!("{error:#}").contains("spritesheetPath must stay inside"));
    }

    #[test]
    fn malformed_animation_reference_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("avatars/chloe-r2-09");
        write_valid_avatar(&existing, "sheet.png");
        fs::write(
            existing.join("avatar.json"),
            r#"{
                "renderMode":"ansi-half-block",
                "spritesheetPath":"sheet.png",
                "frame":{"width":24,"height":24,"columns":1,"rows":1},
                "animations":{"idle":{"frames":[1],"fps":1}}
            }"#,
        )
        .unwrap();

        let error = ensure_bundled_avatars(dir.path()).unwrap_err();

        assert!(error.to_string().contains("avatar pack is invalid"));
        assert!(format!("{error:#}").contains("references frame"));
    }
}
