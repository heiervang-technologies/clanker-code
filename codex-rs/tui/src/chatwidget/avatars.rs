// Modified by Heiervang Technologies.
//! First-class character avatar wiring for the chat surface.

use super::*;
use codex_config::types::TuiPetSide;

impl ChatWidget {
    /// Install one resolved character embodiment without consulting pet config.
    pub(crate) fn set_avatar_binding(
        &mut self,
        binding: crate::avatars::AvatarBinding,
    ) -> anyhow::Result<()> {
        let mut avatar = crate::avatars::AvatarRuntime::load(
            binding,
            self.active_mode_kind(),
            self.frame_requester.clone(),
            self.config.animations,
        )?;
        self.apply_avatar_image_support_override_for_tests(&mut avatar);
        self.ambient_avatar = Some(avatar);
        self.ambient_avatar_image_degraded = false;
        self.sync_ambient_visual_state();
        self.request_redraw();
        Ok(())
    }

    pub(super) fn set_ambient_visual_notification(
        &mut self,
        kind: crate::pets::PetNotificationKind,
        body: Option<String>,
    ) {
        if let Some(avatar) = self.ambient_avatar.as_mut() {
            avatar.set_notification(kind, body.clone());
        }
        if let Some(pet) = self.ambient_pet.as_mut() {
            pet.set_notification(kind, body);
        }
    }

    pub(super) fn sync_ambient_visual_state(&mut self) {
        let mode = self.active_mode_kind();
        if let Some(avatar) = self.ambient_avatar.as_mut()
            && avatar.active_mode() != mode
        {
            match avatar.set_mode(mode) {
                Ok(changed) => {
                    if changed {
                        self.ambient_avatar_image_degraded = false;
                    }
                }
                Err(err) => tracing::warn!(
                    character_id = avatar.character_id(),
                    requested_mode = %mode.display_name(),
                    error = %err,
                    "failed to switch character avatar mode; retaining previous embodiment"
                ),
            }
        }

        if self.ambient_avatar.is_some() || self.ambient_pet.is_some() {
            self.frame_requester
                .schedule_frame_in(crate::pets::TalkingSignal::poll_interval());
        }
        let planning = mode == ModeKind::Plan;
        let playback_talking = (self.ambient_avatar.is_some() || self.ambient_pet.is_some())
            && self.pet_talking_signal.is_active();
        let pet_talking = self.stream_controller.is_some() || playback_talking;
        let context_used_percent = self.token_info.as_ref().and_then(|info| {
            info.model_context_window.map(|window| {
                100 - info
                    .last_token_usage
                    .percent_of_context_window_remaining(window)
                    .clamp(0, 100)
            })
        });
        if let Some(avatar) = self.ambient_avatar.as_mut() {
            avatar.set_planning(planning);
            avatar.set_talking(playback_talking);
            avatar.set_context_used_percent(context_used_percent);
        }
        if let Some(pet) = self.ambient_pet.as_mut() {
            pet.set_planning(planning);
            pet.set_talking(pet_talking);
            pet.set_context_used_percent(context_used_percent);
        }
    }

    pub(crate) fn ambient_avatar_image_enabled(&self) -> bool {
        !self.ambient_avatar_image_degraded
            && self
                .ambient_avatar
                .as_ref()
                .is_some_and(crate::avatars::AvatarRuntime::image_enabled)
    }

    pub(crate) fn ambient_avatar_draw(
        &self,
        area: Rect,
        composer_bottom_y: u16,
    ) -> Option<crate::pets::AmbientPetDraw> {
        if !self.bottom_pane.no_modal_or_popup_active() {
            return None;
        }
        if self.ambient_avatar_image_degraded {
            return None;
        }
        self.ambient_avatar
            .as_ref()?
            .draw_request(area, composer_bottom_y)
    }

    pub(super) fn effective_ambient_avatar_side(&self) -> Option<TuiPetSide> {
        if self.ambient_avatar_image_degraded {
            return None;
        }
        self.ambient_avatar
            .as_ref()
            .filter(|avatar| avatar.visual_enabled())
            .map(|avatar| avatar.placement().as_render_side())
    }

    pub(super) fn resolved_ambient_pet_side(&self, viewport_width: u16) -> Option<TuiPetSide> {
        let pet = self.ambient_pet.as_ref()?;
        if !pet.visual_enabled() {
            return None;
        }
        let requested = self.effective_ambient_pet_side();
        let avatar = self
            .ambient_avatar
            .as_ref()
            .zip(self.effective_ambient_avatar_side())
            .map(|(avatar, side)| (side, avatar.visual_columns()));
        let resolved = crate::avatars::resolve_pet_placement(
            avatar,
            requested,
            pet.visual_columns(),
            viewport_width,
        )?;
        if let Some((avatar_side, avatar_width)) = avatar
            && avatar_side.is_far_side()
            && resolved.is_far_side()
        {
            let combined_reserve = avatar_width
                .saturating_add(AMBIENT_PET_WRAP_GAP_COLUMNS)
                .saturating_add(pet.visual_columns())
                .saturating_add(AMBIENT_PET_WRAP_GAP_COLUMNS);
            if combined_reserve >= viewport_width {
                return None;
            }
        }
        Some(resolved)
    }

    pub(super) fn ambient_visual_horizontal_reserves(&self, width: u16) -> (u16, u16) {
        let mut left = 0;
        let mut right = 0;
        if let (Some(avatar), Some(side)) = (
            self.ambient_avatar.as_ref(),
            self.effective_ambient_avatar_side(),
        ) {
            add_far_side_reserve(side, avatar.visual_columns(), &mut left, &mut right);
        }
        if let (Some(pet), Some(side)) = (
            self.ambient_pet.as_ref(),
            self.resolved_ambient_pet_side(width),
        ) {
            add_far_side_reserve(side, pet.visual_columns(), &mut left, &mut right);
        }
        (left, right)
    }

    pub(super) fn ambient_visual_min_height(&self, width: u16) -> u16 {
        let avatar_height = self
            .ambient_avatar
            .as_ref()
            .zip(self.effective_ambient_avatar_side())
            .filter(|(_, side)| side.is_far_side())
            .map_or(0, |(avatar, _)| avatar.ansi_min_height());
        let pet_height = self
            .ambient_pet
            .as_ref()
            .zip(self.resolved_ambient_pet_side(width))
            .filter(|(_, side)| side.is_far_side())
            .map_or(0, |(pet, _)| pet.ansi_min_height());
        avatar_height.max(pet_height)
    }

    pub(super) fn ambient_visual_band_height(&self, above: bool, width: u16) -> u16 {
        if !self.bottom_pane.no_modal_or_popup_active() {
            return 0;
        }
        let avatar_height = self
            .ambient_avatar
            .as_ref()
            .zip(self.effective_ambient_avatar_side())
            .filter(|(_, side)| side.is_above() == above && !side.is_far_side())
            .map_or(0, |(avatar, _)| avatar.ansi_min_height());
        let pet_height = self
            .ambient_pet
            .as_ref()
            .zip(self.resolved_ambient_pet_side(width))
            .filter(|(_, side)| side.is_above() == above && !side.is_far_side())
            .map_or(0, |(pet, _)| pet.ansi_min_height());
        avatar_height.max(pet_height)
    }

    pub(super) fn render_ambient_visual_band(&self, above: bool, area: Rect, buf: &mut Buffer) {
        if self.ambient_visual_band_height(above, area.width) == 0 {
            return;
        }
        if let Some(avatar) = self.ambient_avatar.as_ref()
            && let Some(side) = self.effective_ambient_avatar_side()
            && side.is_above() == above
            && !side.is_far_side()
        {
            avatar.render_ansi(area, area.bottom(), buf);
        }
        if let Some(pet) = self.ambient_pet.as_ref()
            && let Some(side) = self.resolved_ambient_pet_side(area.width)
            && side.is_above() == above
            && !side.is_far_side()
        {
            pet.render_ansi(area, area.bottom(), side, buf);
        }
    }

    pub(super) fn render_ambient_visual_ansi(&self, area: Rect, buf: &mut Buffer) {
        if !self.bottom_pane.no_modal_or_popup_active() {
            return;
        }
        if let Some(avatar) = self.ambient_avatar.as_ref()
            && self
                .effective_ambient_avatar_side()
                .is_some_and(TuiPetSide::is_far_side)
        {
            avatar.render_ansi(area, area.bottom(), buf);
        }
        if let Some(pet) = self.ambient_pet.as_ref()
            && let Some(side) = self.resolved_ambient_pet_side(area.width)
            && side.is_far_side()
        {
            pet.render_ansi(area, area.bottom(), side, buf);
        }
    }

    #[cfg(test)]
    pub(crate) fn ambient_avatar_active_manifest_for_tests(&self) -> Option<&PathBuf> {
        self.ambient_avatar
            .as_ref()
            .map(crate::avatars::AvatarRuntime::active_manifest)
    }

    #[cfg(test)]
    pub(crate) fn ambient_avatar_semantic_state_for_tests(&self) -> Option<&'static str> {
        self.ambient_avatar
            .as_ref()
            .map(crate::avatars::AvatarRuntime::semantic_animation_name_for_tests)
    }

    #[cfg(test)]
    pub(crate) fn ambient_avatar_character_id_for_tests(&self) -> Option<&str> {
        self.ambient_avatar
            .as_ref()
            .map(crate::avatars::AvatarRuntime::character_id)
    }

    pub(crate) fn degrade_ambient_avatar_image_for_session(&mut self, message: String) {
        self.ambient_avatar_image_degraded = true;
        self.add_warning_message(message);
        self.request_redraw();
    }

    #[cfg(test)]
    fn apply_avatar_image_support_override_for_tests(
        &self,
        avatar: &mut crate::avatars::AvatarRuntime,
    ) {
        if let Some(support) = self.pet_image_support_override {
            avatar.set_image_support_for_tests(support);
        }
    }

    #[cfg(not(test))]
    fn apply_avatar_image_support_override_for_tests(
        &self,
        _avatar: &mut crate::avatars::AvatarRuntime,
    ) {
    }
}

fn add_far_side_reserve(side: TuiPetSide, columns: u16, left: &mut u16, right: &mut u16) {
    let reserve = columns.saturating_add(AMBIENT_PET_WRAP_GAP_COLUMNS);
    match side {
        TuiPetSide::FarLeft => *left = left.saturating_add(reserve),
        TuiPetSide::FarRight => *right = right.saturating_add(reserve),
        _ => {}
    }
}
