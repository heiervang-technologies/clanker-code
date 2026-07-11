//! Chat widget helpers for ambient terminal pets and the pets picker.

use super::*;
use codex_config::types::TuiPetAnchor;
use codex_config::types::TuiPetSide;

pub(super) fn load_ambient_pet(
    config: &Config,
    frame_requester: FrameRequester,
) -> Option<crate::pets::AmbientPet> {
    let selected_pet = config.tui_pet.as_deref()?;
    if selected_pet == crate::pets::DISABLED_PET_ID {
        return None;
    }

    crate::pets::load_with_clanker_fallback(
        selected_pet,
        &config.codex_home,
        frame_requester,
        config.animations,
    )
    .ok()
}

pub(super) fn start_configured_pet_load_if_needed(
    config: &Config,
    ambient_pet_missing: bool,
    frame_requester: FrameRequester,
    app_event_tx: AppEventSender,
) {
    let Some(pet_id) = config.tui_pet.clone() else {
        return;
    };
    if pet_id == crate::pets::DISABLED_PET_ID || !ambient_pet_missing {
        return;
    }

    let codex_home = config.codex_home.clone();
    let animations_enabled = config.animations;
    spawn_pet_load(move || {
        let result = crate::pets::ensure_builtin_pack_for_pet(&pet_id, &codex_home)
            .and_then(|()| {
                crate::pets::load_with_clanker_fallback(
                    &pet_id,
                    &codex_home,
                    frame_requester,
                    animations_enabled,
                )
            })
            .map(Some)
            .map_err(|err| err.to_string());
        app_event_tx.send(AppEvent::ConfiguredPetLoaded { pet_id, result });
    });
}

impl ChatWidget {
    pub(super) fn set_ambient_pet_notification(
        &mut self,
        kind: crate::pets::PetNotificationKind,
        body: Option<String>,
    ) {
        if let Some(pet) = self.ambient_pet.as_mut() {
            pet.set_notification(kind, body);
        }
    }

    pub(crate) fn ambient_pet_image_enabled(&self) -> bool {
        self.ambient_pet
            .as_ref()
            .is_some_and(crate::pets::AmbientPet::image_enabled)
    }

    pub(crate) fn disable_ambient_pet_for_session(&mut self) {
        self.ambient_pet = None;
        self.request_redraw();
    }

    pub(crate) fn ambient_pet_draw(
        &self,
        area: Rect,
        composer_bottom_y: u16,
    ) -> Option<crate::pets::AmbientPetDraw> {
        if !self.bottom_pane.no_modal_or_popup_active() {
            return None;
        }

        let anchor_bottom_y = match self.config.tui_pet_anchor {
            TuiPetAnchor::Composer => composer_bottom_y,
            TuiPetAnchor::ScreenBottom => area.bottom(),
        };
        self.ambient_pet
            .as_ref()?
            .draw_request(area, anchor_bottom_y)
    }

    pub(super) fn ambient_pet_wrap_reserved_cols(&self) -> u16 {
        if !self.effective_ambient_pet_side().is_far_side() {
            return 0;
        }
        self.ambient_pet
            .as_ref()
            .filter(|pet| pet.visual_enabled())
            .map(|pet| {
                pet.visual_columns()
                    .saturating_add(AMBIENT_PET_WRAP_GAP_COLUMNS)
            })
            .unwrap_or(0)
    }

    pub(super) fn ambient_pet_horizontal_reserves(&self) -> (u16, u16) {
        let reserved = self.ambient_pet_wrap_reserved_cols();
        match self.effective_ambient_pet_side() {
            TuiPetSide::FarLeft => (reserved, 0),
            TuiPetSide::FarRight => (0, reserved),
            TuiPetSide::BelowLeft
            | TuiPetSide::BelowCenter
            | TuiPetSide::BelowRight
            | TuiPetSide::AboveLeft
            | TuiPetSide::AboveCenter
            | TuiPetSide::AboveRight => (0, 0),
        }
    }

    pub(super) fn effective_ambient_pet_side(&self) -> TuiPetSide {
        if self
            .ambient_pet
            .as_ref()
            .is_some_and(crate::pets::AmbientPet::ansi_enabled)
        {
            self.config.tui_pet_side
        } else {
            TuiPetSide::FarRight
        }
    }

    pub(super) fn ambient_pet_min_height(&self) -> u16 {
        if !self.effective_ambient_pet_side().is_far_side() {
            return 0;
        }
        self.ambient_pet
            .as_ref()
            .map_or(0, crate::pets::AmbientPet::ansi_min_height)
    }

    pub(super) fn ambient_pet_band_height(&self, placement: TuiPetSide) -> u16 {
        if !self.bottom_pane.no_modal_or_popup_active()
            || self.effective_ambient_pet_side() != placement
        {
            return 0;
        }
        self.ambient_pet
            .as_ref()
            .map_or(0, crate::pets::AmbientPet::ansi_min_height)
    }

    pub(super) fn render_ambient_pet_band(
        &self,
        placement: TuiPetSide,
        area: Rect,
        buf: &mut Buffer,
    ) {
        if self.ambient_pet_band_height(placement) == 0 {
            return;
        }
        if let Some(pet) = self.ambient_pet.as_ref() {
            pet.render_ansi(area, area.bottom(), placement, buf);
        }
    }

    pub(super) fn render_ambient_pet_ansi(&self, area: Rect, buf: &mut Buffer) {
        if !self.bottom_pane.no_modal_or_popup_active()
            || !self.effective_ambient_pet_side().is_far_side()
        {
            return;
        }
        let anchor_bottom_y = area.bottom();
        if let Some(pet) = self.ambient_pet.as_ref() {
            pet.render_ansi(
                area,
                anchor_bottom_y,
                self.effective_ambient_pet_side(),
                buf,
            );
        }
    }

    pub(super) fn render_pet_picker_preview_ansi(&self, buf: &mut Buffer) {
        if self
            .bottom_pane
            .selected_index_for_active_view(crate::pets::PET_PICKER_VIEW_ID)
            .is_none()
        {
            return;
        }
        let Some(area) = self.pet_picker_preview_state.area() else {
            return;
        };
        if let Some(pet) = self.pet_picker_preview_pet.as_ref() {
            pet.render_ansi_preview(area, buf);
        }
    }

    pub(crate) fn history_wrap_width(&self, width: u16) -> u16 {
        width
            .saturating_sub(self.ambient_pet_wrap_reserved_cols())
            .max(1)
    }

    pub(crate) fn history_left_padding(&self) -> u16 {
        self.ambient_pet_horizontal_reserves().0
    }

    pub(crate) fn pet_picker_preview_draw(&self) -> Option<crate::pets::AmbientPetDraw> {
        self.bottom_pane
            .selected_index_for_active_view(crate::pets::PET_PICKER_VIEW_ID)?;
        let area = self.pet_picker_preview_state.area()?;
        let request = self
            .pet_picker_preview_pet
            .as_ref()?
            .preview_draw_request(area)?;
        self.pet_picker_preview_image_visible.set(true);
        Some(request)
    }

    pub(crate) fn should_clear_pet_picker_preview_image(&self) -> bool {
        self.pet_picker_preview_image_visible.replace(false)
    }

    pub(crate) fn fail_pet_picker_preview_render(&mut self, message: String) {
        self.pet_picker_preview_state.set_error(message);
        self.pet_picker_preview_pet = None;
        self.request_redraw();
    }

    pub(crate) fn open_pets_picker(&mut self) {
        self.pet_picker_preview_state.clear();
        self.pet_picker_preview_pet = None;
        self.pet_picker_preview_animation = "idle".to_string();
        let params = crate::pets::build_pet_picker_params(
            self.config.tui_pet.as_deref(),
            &self.config.codex_home,
            self.pet_picker_preview_state.clone(),
        );
        self.bottom_pane.show_selection_view(params);
        let initial_pet_id = self
            .config
            .tui_pet
            .as_deref()
            .unwrap_or(crate::pets::DEFAULT_PET_ID)
            .to_string();
        self.start_pet_picker_preview(initial_pet_id);
    }

    pub(crate) fn select_pet_by_id(&mut self, pet_id: String) {
        self.app_event_tx.send(AppEvent::PetSelected { pet_id });
    }

    pub(crate) fn select_adjacent_pet(&mut self, direction: crate::pets::PetCycleDirection) {
        let Some(pet_id) = crate::pets::adjacent_pet_selector(
            self.config.tui_pet.as_deref(),
            &self.config.codex_home,
            direction,
        ) else {
            return;
        };
        self.select_pet_by_id(pet_id);
    }

    pub(super) fn sync_ambient_pet_semantic_state(&mut self) {
        if self.ambient_pet.is_some() {
            self.frame_requester
                .schedule_frame_in(crate::pets::TalkingSignal::poll_interval());
        }
        let planning = self.active_mode_kind() == ModeKind::Plan;
        let talking = self.stream_controller.is_some()
            || (self.ambient_pet.is_some() && self.pet_talking_signal.is_active());
        let context_used_percent = self.token_info.as_ref().and_then(|info| {
            info.model_context_window.map(|window| {
                100 - info
                    .last_token_usage
                    .percent_of_context_window_remaining(window)
                    .clamp(0, 100)
            })
        });
        if let Some(pet) = self.ambient_pet.as_mut() {
            pet.set_planning(planning);
            pet.set_talking(talking);
            pet.set_context_used_percent(context_used_percent);
        }
    }

    /// Set the pet preselected by the TUI picker in the widget's config copy.
    pub(crate) fn set_tui_pet(&mut self, pet: Option<String>) {
        self.config.tui_pet = pet;
        self.ambient_pet = load_ambient_pet(&self.config, self.frame_requester.clone());
        self.sync_ambient_pet_semantic_state();
        self.apply_ambient_pet_image_support_override_for_tests();
        self.request_redraw();
    }

    pub(crate) fn set_tui_pet_side(&mut self, side: TuiPetSide) {
        self.config.tui_pet_side = side;
        self.request_redraw();
    }

    pub(crate) fn set_tui_pet_loaded(
        &mut self,
        pet: Option<String>,
        ambient_pet: Option<crate::pets::AmbientPet>,
    ) {
        self.config.tui_pet = pet;
        self.ambient_pet = ambient_pet;
        self.sync_ambient_pet_semantic_state();
        self.apply_ambient_pet_image_support_override_for_tests();
        self.request_redraw();
    }

    #[cfg(test)]
    fn apply_ambient_pet_image_support_override_for_tests(&mut self) {
        if let Some(support) = self.pet_image_support_override
            && let Some(pet) = self.ambient_pet.as_mut()
        {
            pet.set_image_support_for_tests(support);
        }
    }

    #[cfg(not(test))]
    fn apply_ambient_pet_image_support_override_for_tests(&mut self) {}

    #[cfg(test)]
    fn apply_pet_image_support_override_for_tests(&self, pet: &mut crate::pets::AmbientPet) {
        if let Some(support) = self.pet_image_support_override {
            pet.set_image_support_for_tests(support);
        }
    }

    #[cfg(not(test))]
    fn apply_pet_image_support_override_for_tests(&self, _pet: &mut crate::pets::AmbientPet) {}

    pub(crate) fn start_pet_picker_preview(&mut self, pet_id: String) {
        self.pet_picker_preview_request_id =
            self.pet_picker_preview_request_id.wrapping_add(/*rhs*/ 1);
        let request_id = self.pet_picker_preview_request_id;
        self.pet_picker_preview_pet = None;
        if pet_id == crate::pets::DISABLED_PET_ID {
            self.pet_picker_preview_state.set_disabled();
            self.request_redraw();
            return;
        }

        self.pet_picker_preview_state.set_loading();
        self.request_redraw();

        let codex_home = self.config.codex_home.clone();
        let frame_requester = self.frame_requester.clone();
        let animations_enabled = self.config.animations;
        let tx = self.app_event_tx.clone();
        spawn_pet_load(move || {
            let result = crate::pets::ensure_builtin_pack_for_pet(&pet_id, &codex_home)
                .and_then(|()| {
                    crate::pets::AmbientPet::load(
                        Some(&pet_id),
                        &codex_home,
                        frame_requester,
                        animations_enabled,
                    )
                })
                .map_err(|err| err.to_string());
            tx.send(AppEvent::PetPreviewLoaded { request_id, result });
        });
    }

    pub(crate) fn finish_pet_picker_preview_load(
        &mut self,
        request_id: u64,
        result: Result<crate::pets::AmbientPet, String>,
    ) {
        if request_id != self.pet_picker_preview_request_id {
            return;
        }

        match result {
            Ok(mut pet) => {
                self.apply_pet_image_support_override_for_tests(&mut pet);
                pet.set_preview_animation(&self.pet_picker_preview_animation);
                if let Some(message) = pet.unavailable_message() {
                    self.pet_picker_preview_state.set_error(message.to_string());
                    self.pet_picker_preview_pet = None;
                    self.request_redraw();
                    return;
                }
                self.pet_picker_preview_state.set_ready();
                self.pet_picker_preview_pet = Some(pet);
            }
            Err(message) => {
                self.pet_picker_preview_state.set_error(message);
                self.pet_picker_preview_pet = None;
            }
        }
        self.request_redraw();
    }

    pub(crate) fn set_pet_picker_preview_animation(&mut self, animation_name: String) {
        self.pet_picker_preview_animation = animation_name;
        if let Some(pet) = self.pet_picker_preview_pet.as_mut() {
            pet.set_preview_animation(&self.pet_picker_preview_animation);
        }
        self.request_redraw();
    }

    pub(crate) fn show_pet_selection_loading_popup(&mut self) -> u64 {
        self.pet_selection_load_request_id =
            self.pet_selection_load_request_id.wrapping_add(/*rhs*/ 1);
        self.pet_picker_preview_state.clear();
        self.pet_picker_preview_pet = None;
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(PET_SELECTION_LOADING_VIEW_ID),
            title: Some("Loading Pet".to_string()),
            subtitle: Some("Preparing the terminal pet.".to_string()),
            items: vec![SelectionItem {
                name: "Loading selected pet...".to_string(),
                is_disabled: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        self.pet_selection_load_request_id
    }

    pub(crate) fn finish_pet_selection_loading_popup(&mut self, request_id: u64) -> bool {
        if request_id != self.pet_selection_load_request_id {
            return false;
        }
        self.bottom_pane
            .dismiss_active_view_if_id(PET_SELECTION_LOADING_VIEW_ID);
        true
    }

    #[cfg(test)]
    pub(crate) fn set_pet_image_support_for_tests(
        &mut self,
        support: crate::pets::PetImageSupport,
    ) {
        self.pet_image_support_override = Some(support);
        self.apply_ambient_pet_image_support_override_for_tests();
    }

    #[cfg(test)]
    pub(crate) fn install_test_ambient_pet_for_tests(&mut self, animations_enabled: bool) {
        self.set_tui_pet_loaded(
            Some("test".to_string()),
            Some(crate::pets::test_ambient_pet(
                self.frame_requester.clone(),
                animations_enabled,
            )),
        );
    }
}

fn spawn_pet_load(f: impl FnOnce() + Send + 'static) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::mem::drop(handle.spawn_blocking(f));
    } else {
        let _ = std::thread::spawn(f);
    }
}
