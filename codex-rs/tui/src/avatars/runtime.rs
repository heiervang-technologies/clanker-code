use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use codex_character::ValidatedAvatarPack;
use codex_config::types::TuiPetSide;
use codex_protocol::config_types::ModeKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::pets::AmbientPet;
use crate::pets::AmbientPetDraw;
use crate::pets::PetNotificationKind;
use crate::tui::FrameRequester;

/// Renderer-owned placement independent of the legacy pet config schema.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AvatarPlacement {
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

impl AvatarPlacement {
    pub(crate) fn as_render_side(self) -> TuiPetSide {
        match self {
            Self::FarLeft => TuiPetSide::FarLeft,
            Self::FarRight => TuiPetSide::FarRight,
            Self::AboveLeft => TuiPetSide::AboveLeft,
            Self::AboveCenter => TuiPetSide::AboveCenter,
            Self::AboveRight => TuiPetSide::AboveRight,
            Self::BelowLeft => TuiPetSide::BelowLeft,
            Self::BelowCenter => TuiPetSide::BelowCenter,
            Self::BelowRight => TuiPetSide::BelowRight,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::FarLeft => "far_left",
            Self::FarRight => "far_right",
            Self::AboveLeft => "above_left",
            Self::AboveCenter => "above_center",
            Self::AboveRight => "above_right",
            Self::BelowLeft => "below_left",
            Self::BelowCenter => "below_center",
            Self::BelowRight => "below_right",
        }
    }
}

/// Fully validated avatar packs for one canonical character.
///
/// Core validates selectors, assets, geometry, and normalized animations. The
/// renderer consumes that result without reparsing the character package.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AvatarBinding {
    character_id: String,
    default_pack: ValidatedAvatarPack,
    by_mode: HashMap<ModeKind, ValidatedAvatarPack>,
    placement: AvatarPlacement,
    cache_root: PathBuf,
}

impl AvatarBinding {
    pub(crate) fn new(
        character_id: String,
        default_pack: ValidatedAvatarPack,
        by_mode: HashMap<ModeKind, ValidatedAvatarPack>,
        placement: AvatarPlacement,
        cache_root: PathBuf,
    ) -> Self {
        Self {
            character_id,
            default_pack,
            by_mode,
            placement,
            cache_root,
        }
    }

    pub(crate) fn character_id(&self) -> &str {
        &self.character_id
    }

    pub(crate) fn placement(&self) -> AvatarPlacement {
        self.placement
    }

    #[cfg(test)]
    pub(crate) fn manifest_for_mode(&self, mode: ModeKind) -> &PathBuf {
        &self.pack_for_mode(mode).manifest_path
    }

    #[cfg(test)]
    pub(crate) fn default_manifest(&self) -> &PathBuf {
        &self.default_pack.manifest_path
    }

    fn pack_for_mode(&self, mode: ModeKind) -> &ValidatedAvatarPack {
        self.by_mode.get(&mode).unwrap_or(&self.default_pack)
    }

    fn default_pack(&self) -> &ValidatedAvatarPack {
        &self.default_pack
    }

    fn cache_root(&self) -> &std::path::Path {
        &self.cache_root
    }
}

/// One mandatory character embodiment with mode-aware asset selection.
#[derive(Debug)]
pub(crate) struct AvatarRuntime {
    binding: AvatarBinding,
    active_mode: ModeKind,
    active_manifest: PathBuf,
    visual: AmbientPet,
    frame_requester: FrameRequester,
    animations_enabled: bool,
}

impl AvatarRuntime {
    pub(crate) fn load(
        binding: AvatarBinding,
        mode: ModeKind,
        frame_requester: FrameRequester,
        animations_enabled: bool,
    ) -> Result<Self> {
        let requested_pack = binding.pack_for_mode(mode).clone();
        let (active_manifest, visual) = match load_visual_for_placement(
            &requested_pack,
            frame_requester.clone(),
            animations_enabled,
            binding.placement(),
            binding.cache_root(),
        ) {
            Ok(visual) => (requested_pack.manifest_path, visual),
            Err(override_error)
                if requested_pack.manifest_path != binding.default_pack().manifest_path =>
            {
                let fallback = binding.default_pack().clone();
                let visual = load_visual_for_placement(
                    &fallback,
                    frame_requester.clone(),
                    animations_enabled,
                    binding.placement(),
                    binding.cache_root(),
                )
                .with_context(|| {
                    format!(
                        "avatar mode override {} failed ({override_error:#}); default fallback also failed",
                        requested_pack.manifest_path.display()
                    )
                })?;
                (fallback.manifest_path, visual)
            }
            Err(error) => return Err(error),
        };
        Ok(Self {
            binding,
            active_mode: mode,
            active_manifest,
            visual,
            frame_requester,
            animations_enabled,
        })
    }

    pub(crate) fn character_id(&self) -> &str {
        self.binding.character_id()
    }

    #[cfg(test)]
    pub(crate) fn active_manifest(&self) -> &PathBuf {
        &self.active_manifest
    }

    pub(crate) fn active_mode(&self) -> ModeKind {
        self.active_mode
    }

    pub(crate) fn placement(&self) -> AvatarPlacement {
        self.binding.placement()
    }

    /// Switch mode variants transactionally, retaining the current visual when
    /// a replacement cannot be loaded and preserving lifecycle animation state.
    pub(crate) fn set_mode(&mut self, mode: ModeKind) -> Result<bool> {
        let requested_pack = self.binding.pack_for_mode(mode).clone();
        if requested_pack.manifest_path == self.active_manifest {
            self.active_mode = mode;
            return Ok(false);
        }

        let loaded = load_visual_for_placement(
            &requested_pack,
            self.frame_requester.clone(),
            self.animations_enabled,
            self.binding.placement(),
            self.binding.cache_root(),
        );
        let (next_manifest, mut next) = match loaded {
            Ok(next) => (requested_pack.manifest_path, next),
            Err(override_error)
                if requested_pack.manifest_path != self.binding.default_pack().manifest_path =>
            {
                let fallback = self.binding.default_pack().clone();
                if fallback.manifest_path == self.active_manifest {
                    self.active_mode = mode;
                    return Ok(false);
                }
                let next = load_visual_for_placement(
                    &fallback,
                    self.frame_requester.clone(),
                    self.animations_enabled,
                    self.binding.placement(),
                    self.binding.cache_root(),
                )
                .with_context(|| {
                    format!(
                        "avatar mode override {} failed ({override_error:#}); default fallback also failed",
                        requested_pack.manifest_path.display()
                    )
                })?;
                (fallback.manifest_path, next)
            }
            Err(error) => return Err(error),
        };
        next.inherit_runtime_state_from(&self.visual);
        self.visual = next;
        self.active_manifest = next_manifest;
        self.active_mode = mode;
        Ok(true)
    }

    pub(crate) fn set_notification(&mut self, kind: PetNotificationKind, body: Option<String>) {
        self.visual.set_notification(kind, body);
    }

    pub(crate) fn set_planning(&mut self, planning: bool) {
        self.visual.set_planning(planning);
    }

    pub(crate) fn set_talking(&mut self, talking: bool) {
        self.visual.set_talking(talking);
    }

    pub(crate) fn set_context_used_percent(&mut self, used_percent: Option<i64>) {
        self.visual.set_context_used_percent(used_percent);
    }

    pub(crate) fn image_enabled(&self) -> bool {
        self.visual.image_enabled()
    }

    pub(crate) fn visual_enabled(&self) -> bool {
        self.visual.visual_enabled()
    }

    pub(crate) fn visual_columns(&self) -> u16 {
        self.visual.visual_columns()
    }

    pub(crate) fn ansi_min_height(&self) -> u16 {
        self.visual.ansi_min_height()
    }

    pub(crate) fn schedule_next_frame(&self) {
        self.visual.schedule_next_frame();
    }

    #[cfg(test)]
    pub(crate) fn set_image_support_for_tests(&mut self, support: crate::pets::PetImageSupport) {
        self.visual.set_image_support_for_tests(support);
    }

    pub(crate) fn draw_request(
        &self,
        area: Rect,
        composer_bottom_y: u16,
    ) -> Option<AmbientPetDraw> {
        self.visual
            .draw_request_at_side(area, composer_bottom_y, self.placement().as_render_side())
    }

    pub(crate) fn render_ansi(&self, area: Rect, anchor_bottom_y: u16, buf: &mut Buffer) {
        self.visual.render_ansi(
            area,
            anchor_bottom_y,
            self.placement().as_render_side(),
            buf,
        );
    }

    #[cfg(test)]
    pub(crate) fn semantic_animation_name_for_tests(&self) -> &'static str {
        self.visual.semantic_animation_name_for_tests()
    }

    #[cfg(test)]
    fn animation_started_at(&self) -> std::time::Instant {
        self.visual.animation_started_at_for_tests()
    }
}

fn load_visual(
    pack: &ValidatedAvatarPack,
    cache_root: &std::path::Path,
    frame_requester: FrameRequester,
    animations_enabled: bool,
) -> Result<AmbientPet> {
    AmbientPet::from_validated_avatar_pack(pack, cache_root, frame_requester, animations_enabled)
}

fn load_visual_for_placement(
    pack: &ValidatedAvatarPack,
    frame_requester: FrameRequester,
    animations_enabled: bool,
    placement: AvatarPlacement,
    cache_root: &std::path::Path,
) -> Result<AmbientPet> {
    let visual = load_visual(pack, cache_root, frame_requester, animations_enabled)?;
    if visual.uses_terminal_image() && !placement.as_render_side().is_far_side() {
        anyhow::bail!(
            "terminal-image character avatars support only far_left or far_right placement; got {}",
            placement.name()
        );
    }
    Ok(visual)
}

/// Resolve a pet's transient render slot without mutating its preference.
pub(crate) fn resolve_pet_placement(
    avatar: Option<(TuiPetSide, u16)>,
    requested_pet: TuiPetSide,
    pet_width: u16,
    viewport_width: u16,
) -> Option<TuiPetSide> {
    let Some((avatar_side, avatar_width)) = avatar else {
        return Some(requested_pet);
    };
    if band(avatar_side) != band(requested_pet)
        || slots_are_disjoint(
            avatar_side,
            avatar_width,
            requested_pet,
            pet_width,
            viewport_width,
        )
    {
        return Some(requested_pet);
    }

    collision_fallbacks(requested_pet)
        .into_iter()
        .find(|candidate| {
            slots_are_disjoint(
                avatar_side,
                avatar_width,
                *candidate,
                pet_width,
                viewport_width,
            )
        })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PlacementBand {
    Far,
    Above,
    Below,
}

fn band(side: TuiPetSide) -> PlacementBand {
    match side {
        TuiPetSide::FarLeft | TuiPetSide::FarRight => PlacementBand::Far,
        TuiPetSide::AboveLeft | TuiPetSide::AboveCenter | TuiPetSide::AboveRight => {
            PlacementBand::Above
        }
        TuiPetSide::BelowLeft | TuiPetSide::BelowCenter | TuiPetSide::BelowRight => {
            PlacementBand::Below
        }
    }
}

fn collision_fallbacks(side: TuiPetSide) -> Vec<TuiPetSide> {
    match side {
        TuiPetSide::FarLeft => vec![TuiPetSide::FarRight],
        TuiPetSide::FarRight => vec![TuiPetSide::FarLeft],
        TuiPetSide::AboveLeft => vec![TuiPetSide::AboveCenter, TuiPetSide::AboveRight],
        TuiPetSide::AboveCenter => vec![TuiPetSide::AboveLeft, TuiPetSide::AboveRight],
        TuiPetSide::AboveRight => vec![TuiPetSide::AboveCenter, TuiPetSide::AboveLeft],
        TuiPetSide::BelowLeft => vec![TuiPetSide::BelowCenter, TuiPetSide::BelowRight],
        TuiPetSide::BelowCenter => vec![TuiPetSide::BelowLeft, TuiPetSide::BelowRight],
        TuiPetSide::BelowRight => vec![TuiPetSide::BelowCenter, TuiPetSide::BelowLeft],
    }
}

fn slots_are_disjoint(
    avatar_side: TuiPetSide,
    avatar_width: u16,
    pet_side: TuiPetSide,
    pet_width: u16,
    viewport_width: u16,
) -> bool {
    let Some((avatar_start, avatar_end)) =
        horizontal_span(avatar_side, avatar_width, viewport_width)
    else {
        return false;
    };
    let Some((pet_start, pet_end)) = horizontal_span(pet_side, pet_width, viewport_width) else {
        return false;
    };
    avatar_end <= pet_start || pet_end <= avatar_start
}

fn horizontal_span(side: TuiPetSide, width: u16, viewport_width: u16) -> Option<(u16, u16)> {
    if width == 0 || width > viewport_width {
        return None;
    }
    let start = match side {
        TuiPetSide::FarLeft | TuiPetSide::AboveLeft | TuiPetSide::BelowLeft => 0,
        TuiPetSide::AboveCenter | TuiPetSide::BelowCenter => {
            viewport_width.saturating_sub(width) / 2
        }
        TuiPetSide::FarRight | TuiPetSide::AboveRight | TuiPetSide::BelowRight => {
            viewport_width.saturating_sub(width)
        }
    };
    Some((start, start.saturating_add(width)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_character::AvatarSelector;
    use codex_character::validate_avatar_selector;

    fn write_avatar(path: &std::path::Path, color: [u8; 4]) {
        std::fs::create_dir_all(path).unwrap();
        image::RgbaImage::from_pixel(24, 24, image::Rgba(color))
            .save(path.join("sheet.png"))
            .unwrap();
        std::fs::write(
            path.join("avatar.json"),
            r#"{
                "renderMode":"ansi-half-block",
                "spritesheetPath":"sheet.png",
                "frame":{"width":24,"height":24,"columns":1,"rows":1},
                "animations":{
                    "idle":{"frames":[0],"fps":1},
                    "planning":{"frames":[0],"fps":1}
                }
            }"#,
        )
        .unwrap();
    }

    fn write_terminal_image_avatar(path: &std::path::Path) {
        std::fs::create_dir_all(path).unwrap();
        image::RgbaImage::new(192 * 8, 208 * 9)
            .save(path.join("sheet.png"))
            .unwrap();
        std::fs::write(
            path.join("avatar.json"),
            r#"{
                "renderMode":"terminal-image",
                "spritesheetPath":"sheet.png"
            }"#,
        )
        .unwrap();
    }

    fn validated_pack(root: &std::path::Path, id: &str) -> ValidatedAvatarPack {
        validate_avatar_selector(root, &AvatarSelector(format!("{id}/avatar.json"))).unwrap()
    }

    fn canonical_manifest(root: &std::path::Path, id: &str) -> PathBuf {
        root.join(id).join("avatar.json").canonicalize().unwrap()
    }

    #[test]
    fn mode_override_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        write_avatar(&dir.path().join("default"), [0, 255, 0, 255]);
        write_avatar(&dir.path().join("locked-in"), [255, 0, 0, 255]);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked-in"))]),
            AvatarPlacement::FarRight,
            dir.path().to_path_buf(),
        );

        assert_eq!(
            binding
                .manifest_for_mode(ModeKind::LockedIn)
                .strip_prefix(dir.path())
                .unwrap(),
            std::path::Path::new("locked-in/avatar.json")
        );
        assert_eq!(
            binding
                .manifest_for_mode(ModeKind::Larp)
                .strip_prefix(dir.path())
                .unwrap(),
            std::path::Path::new("default/avatar.json")
        );
        assert_eq!(binding.character_id(), "chloe");
    }

    #[test]
    fn colliding_pet_moves_to_nearest_safe_slot_without_changing_request() {
        let requested = TuiPetSide::BelowRight;

        let resolved = resolve_pet_placement(Some((TuiPetSide::BelowRight, 24)), requested, 12, 80);

        assert_eq!(resolved, Some(TuiPetSide::BelowCenter));
        assert_eq!(requested, TuiPetSide::BelowRight);
    }

    #[test]
    fn pet_hides_when_same_band_has_no_safe_slot() {
        assert_eq!(
            resolve_pet_placement(
                Some((TuiPetSide::AboveCenter, 24)),
                TuiPetSide::AboveCenter,
                24,
                30,
            ),
            None
        );
    }

    #[test]
    fn pet_keeps_requested_slot_in_another_band() {
        assert_eq!(
            resolve_pet_placement(
                Some((TuiPetSide::FarRight, 24)),
                TuiPetSide::BelowRight,
                24,
                30,
            ),
            Some(TuiPetSide::BelowRight)
        );
    }

    #[test]
    fn adjacent_exact_fit_slots_do_not_collide() {
        assert_eq!(
            resolve_pet_placement(
                Some((TuiPetSide::FarLeft, 24)),
                TuiPetSide::FarRight,
                24,
                48,
            ),
            Some(TuiPetSide::FarRight)
        );
    }

    #[test]
    fn startup_invalid_override_falls_back_to_valid_default() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        let locked = dir.path().join("locked");
        write_avatar(&default, [0, 255, 0, 255]);
        write_avatar(&locked, [255, 0, 0, 255]);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked"))]),
            AvatarPlacement::FarRight,
            dir.path().to_path_buf(),
        );
        std::fs::remove_file(locked.join("sheet.png")).unwrap();

        let runtime = AvatarRuntime::load(
            binding,
            ModeKind::LockedIn,
            FrameRequester::test_dummy(),
            false,
        )
        .unwrap();

        assert_eq!(runtime.active_mode(), ModeKind::LockedIn);
        assert_eq!(
            runtime.active_manifest(),
            &canonical_manifest(dir.path(), "default")
        );
        assert!(dir.path().join("cache/tui-pets/frame-cache").is_dir());
        assert!(!default.join("cache").exists());
    }

    #[test]
    fn initial_terminal_image_rejects_non_far_placement() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        write_terminal_image_avatar(&default);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::new(),
            AvatarPlacement::BelowRight,
            dir.path().to_path_buf(),
        );

        let error =
            AvatarRuntime::load(binding, ModeKind::Larp, FrameRequester::test_dummy(), false)
                .unwrap_err();

        assert!(
            format!("{error:#}").contains(
                "terminal-image character avatars support only far_left or far_right placement; got below_right"
            )
        );
    }

    #[test]
    fn invalid_terminal_image_mode_override_retains_default_pack() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        let locked = dir.path().join("locked");
        write_avatar(&default, [0, 255, 0, 255]);
        write_terminal_image_avatar(&locked);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked"))]),
            AvatarPlacement::BelowRight,
            dir.path().to_path_buf(),
        );
        let mut runtime =
            AvatarRuntime::load(binding, ModeKind::Larp, FrameRequester::test_dummy(), false)
                .unwrap();

        let changed = runtime.set_mode(ModeKind::LockedIn).unwrap();

        assert!(!changed);
        assert_eq!(runtime.active_mode(), ModeKind::LockedIn);
        assert_eq!(
            runtime.active_manifest(),
            &canonical_manifest(dir.path(), "default")
        );
    }

    #[test]
    fn failed_mode_switch_preserves_runtime_transactionally() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        let locked = dir.path().join("locked");
        write_avatar(&default, [0, 255, 0, 255]);
        write_avatar(&locked, [255, 0, 0, 255]);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked"))]),
            AvatarPlacement::FarRight,
            dir.path().to_path_buf(),
        );
        let mut runtime = AvatarRuntime::load(
            binding,
            ModeKind::LockedIn,
            FrameRequester::test_dummy(),
            false,
        )
        .unwrap();
        runtime.set_planning(true);
        runtime.set_talking(true);
        let before_epoch = runtime.animation_started_at();
        std::fs::remove_file(default.join("sheet.png")).unwrap();

        let error = runtime.set_mode(ModeKind::Larp).unwrap_err();

        assert!(error.to_string().contains("default"));
        assert_eq!(runtime.active_mode(), ModeKind::LockedIn);
        assert_eq!(
            runtime.active_manifest(),
            &canonical_manifest(dir.path(), "locked")
        );
        assert_eq!(runtime.semantic_animation_name_for_tests(), "talking");
        assert_eq!(runtime.animation_started_at(), before_epoch);
    }

    #[test]
    fn invalid_override_uses_default_and_preserves_semantic_state() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        let locked = dir.path().join("locked");
        write_avatar(&default, [0, 255, 0, 255]);
        write_avatar(&locked, [255, 0, 0, 255]);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked"))]),
            AvatarPlacement::FarRight,
            dir.path().to_path_buf(),
        );
        let mut runtime =
            AvatarRuntime::load(binding, ModeKind::Larp, FrameRequester::test_dummy(), false)
                .unwrap();
        runtime.set_planning(true);
        std::fs::remove_file(locked.join("sheet.png")).unwrap();

        let changed = runtime.set_mode(ModeKind::LockedIn).unwrap();

        assert!(!changed);
        assert_eq!(runtime.active_mode(), ModeKind::LockedIn);
        assert_eq!(
            runtime.active_manifest(),
            &canonical_manifest(dir.path(), "default")
        );
        assert_eq!(runtime.semantic_animation_name_for_tests(), "planning");
    }

    #[test]
    fn valid_mode_swap_preserves_semantic_state() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("default");
        let locked = dir.path().join("locked");
        write_avatar(&default, [0, 255, 0, 255]);
        write_avatar(&locked, [255, 0, 0, 255]);
        let binding = AvatarBinding::new(
            "chloe".to_string(),
            validated_pack(dir.path(), "default"),
            HashMap::from([(ModeKind::LockedIn, validated_pack(dir.path(), "locked"))]),
            AvatarPlacement::FarRight,
            dir.path().to_path_buf(),
        );
        let mut runtime =
            AvatarRuntime::load(binding, ModeKind::Larp, FrameRequester::test_dummy(), false)
                .unwrap();
        runtime.set_planning(true);

        let changed = runtime.set_mode(ModeKind::LockedIn).unwrap();

        assert!(changed);
        assert_eq!(
            runtime.active_manifest(),
            &canonical_manifest(dir.path(), "locked")
        );
        assert_eq!(runtime.semantic_animation_name_for_tests(), "planning");
    }
}
