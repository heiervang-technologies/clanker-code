use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use codex_character::AvatarSelector;
use codex_character::CharacterCatalog;
use codex_character::ResolvedCharacter;
use codex_character::ValidatedAvatarPack;
use codex_character::validate_avatar_selector;

use super::AvatarBinding;
use super::AvatarPlacement;

pub(crate) fn resolve_named_avatar_binding(
    codex_home: &Path,
    requested_name: &str,
) -> Result<AvatarBinding> {
    super::assets::ensure_bundled_character_for_name(codex_home, requested_name)?;
    let resolved = CharacterCatalog::load(codex_home)
        .resolve(requested_name)
        .map_err(|issues| {
            let details = issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::anyhow!("failed to resolve character {requested_name:?}: {details}")
        })?;
    binding_from_resolved_character(&resolved)
}

pub(crate) fn binding_from_resolved_character(
    resolved: &ResolvedCharacter,
) -> Result<AvatarBinding> {
    let default_pack =
        resolve_avatar_pack(&resolved.package_root, &resolved.manifest.avatar, "avatar")?;
    let by_mode = resolved
        .manifest
        .avatar_by_mode
        .iter()
        .map(|(mode, selector)| {
            resolve_avatar_pack(
                &resolved.package_root,
                selector,
                &format!("avatarByMode.{}", mode.display_name()),
            )
            .map(|manifest| (*mode, manifest))
        })
        .collect::<Result<HashMap<_, _>>>()?;

    Ok(AvatarBinding::new(
        resolved.manifest.id.clone(),
        default_pack,
        by_mode,
        resolved.manifest.avatar_placement.into(),
        character_cache_root(&resolved.package_root)?,
    ))
}

fn character_cache_root(package_root: &Path) -> Result<std::path::PathBuf> {
    package_root
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .with_context(|| {
            format!(
                "character package is not under CODEX_HOME/characters: {}",
                package_root.display()
            )
        })
}

fn resolve_avatar_pack(
    package_root: &Path,
    selector: &AvatarSelector,
    field: &str,
) -> Result<ValidatedAvatarPack> {
    validate_avatar_selector(package_root, selector).with_context(|| {
        format!(
            "validate {field} avatar selector {} in {}",
            selector.as_str(),
            package_root.display()
        )
    })
}

impl From<codex_character::AvatarPlacement> for AvatarPlacement {
    fn from(value: codex_character::AvatarPlacement) -> Self {
        match value {
            codex_character::AvatarPlacement::FarLeft => Self::FarLeft,
            codex_character::AvatarPlacement::FarRight => Self::FarRight,
            codex_character::AvatarPlacement::AboveLeft => Self::AboveLeft,
            codex_character::AvatarPlacement::AboveCenter => Self::AboveCenter,
            codex_character::AvatarPlacement::AboveRight => Self::AboveRight,
            codex_character::AvatarPlacement::BelowLeft => Self::BelowLeft,
            codex_character::AvatarPlacement::BelowCenter => Self::BelowCenter,
            codex_character::AvatarPlacement::BelowRight => Self::BelowRight,
        }
    }
}

#[cfg(test)]
mod tests {
    use codex_protocol::config_types::ModeKind;

    use super::*;

    #[test]
    fn bundled_chloe_resolves_to_canonical_mode_aware_binding() {
        let home = tempfile::tempdir().unwrap();

        let binding = resolve_named_avatar_binding(home.path(), "ChLoE").unwrap();

        assert_eq!(binding.character_id(), "chloe");
        assert_eq!(binding.placement(), AvatarPlacement::FarRight);
        assert!(
            binding
                .default_manifest()
                .ends_with("characters/chloe/avatar/default/avatar.json")
        );
        assert!(
            binding
                .manifest_for_mode(ModeKind::LockedIn)
                .ends_with("characters/chloe/avatar/locked-in/avatar.json")
        );
        assert_eq!(
            binding.manifest_for_mode(ModeKind::Plan),
            binding.default_manifest()
        );
    }

    #[test]
    fn named_binding_ignores_unrelated_invalid_character_packages() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("characters/clanker")).unwrap();
        let legacy_clanker = home.path().join("avatars/clanker");
        std::fs::create_dir_all(&legacy_clanker).unwrap();
        std::fs::write(
            legacy_clanker.join("avatar.json"),
            r#"{"spritesheetPath":"missing.png"}"#,
        )
        .unwrap();
        let malformed = home.path().join("characters/malformed");
        std::fs::create_dir_all(&malformed).unwrap();
        std::fs::write(malformed.join("character.json"), "{").unwrap();
        let missing_avatar = home.path().join("characters/missing-avatar");
        std::fs::create_dir_all(&missing_avatar).unwrap();
        std::fs::write(
            missing_avatar.join("character.json"),
            r#"{
                "schemaVersion":1,
                "id":"missing-avatar",
                "displayName":"Missing Avatar",
                "avatar":"avatar/default/avatar.json"
            }"#,
        )
        .unwrap();

        let binding = resolve_named_avatar_binding(home.path(), "cleo").unwrap();

        assert_eq!(binding.character_id(), "chloe");
    }

    #[test]
    fn selected_partial_bundled_character_fails_during_named_resolution() {
        let home = tempfile::tempdir().unwrap();
        super::super::assets::ensure_bundled_avatars(home.path()).unwrap();
        std::fs::remove_file(
            home.path()
                .join("characters/chloe/avatar/default/sheet.png"),
        )
        .unwrap();

        let error = resolve_named_avatar_binding(home.path(), "chloe").unwrap_err();

        let detail = format!("{error:#}");
        assert!(detail.contains("failed to resolve character \"chloe\""));
        assert!(detail.contains("avatar"));
    }

    #[test]
    fn custom_name_does_not_materialize_unrelated_bundled_characters() {
        let home = tempfile::tempdir().unwrap();
        let package = home.path().join("characters/orion");
        let avatar = package.join("avatar/default");
        std::fs::create_dir_all(&avatar).unwrap();
        image::RgbaImage::new(24, 24)
            .save(avatar.join("sheet.png"))
            .unwrap();
        std::fs::write(
            avatar.join("avatar.json"),
            r#"{
                "renderMode":"ansi-half-block",
                "spritesheetPath":"sheet.png",
                "frame":{"width":24,"height":24,"columns":1,"rows":1},
                "animations":{"idle":{"frames":[0],"fps":1}}
            }"#,
        )
        .unwrap();
        std::fs::write(
            package.join("character.json"),
            r#"{
                "schemaVersion":1,
                "id":"orion",
                "displayName":"Orion",
                "avatar":"avatar/default/avatar.json"
            }"#,
        )
        .unwrap();

        let binding = resolve_named_avatar_binding(home.path(), "orion").unwrap();

        assert_eq!(binding.character_id(), "orion");
        assert!(!home.path().join("characters/chloe").exists());
        assert!(!home.path().join("characters/clanker").exists());
        assert!(!home.path().join("avatars").exists());
    }

    #[cfg(unix)]
    #[test]
    fn avatar_selector_symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let package = home.path().join("characters/escape");
        let outside = home.path().join("outside");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        image::RgbaImage::new(24, 24)
            .save(outside.join("sheet.png"))
            .unwrap();
        std::fs::write(
            outside.join("avatar.json"),
            r#"{
                "renderMode":"ansi-half-block",
                "spritesheetPath":"sheet.png",
                "frame":{"width":24,"height":24,"columns":1,"rows":1},
                "animations":{"idle":{"frames":[0],"fps":1}}
            }"#,
        )
        .unwrap();
        symlink(&outside, package.join("avatar")).unwrap();
        std::fs::write(
            package.join("character.json"),
            r#"{
                "schemaVersion":1,
                "id":"escape",
                "displayName":"Escape",
                "avatar":"avatar/avatar.json"
            }"#,
        )
        .unwrap();
        let error = resolve_named_avatar_binding(home.path(), "escape").unwrap_err();

        assert!(format!("{error:#}").contains("escapes the character package"));
    }
}
