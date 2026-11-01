//! TUI model and collaboration inventories; refreshing models preserves the server mode catalog.

use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::config_types::CollaborationModeOverride;
use codex_protocol::config_types::ModeKind;
use codex_protocol::openai_models::ModelPreset;
use std::collections::HashMap;
use std::convert::Infallible;

pub(crate) const LUNA_RESERVE_MODEL: &str = "gpt-reserve";
pub(crate) const LUNA_MODEL: &str = "gpt-6-luna";

pub(crate) fn model_display_name(model: &str) -> &str {
    if model.eq_ignore_ascii_case(LUNA_RESERVE_MODEL) {
        "Luna Reserve"
    } else {
        model
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ModelCatalog {
    pub(crate) models: Vec<ModelPreset>,
    pub(crate) collaboration_modes: Vec<CollaborationModeMask>,
    /// Clanker per-mode overrides from `[collaboration_modes]`, applied on top of the
    /// server-discovered mode catalog.
    collaboration_mode_overrides: HashMap<ModeKind, CollaborationModeOverride>,
}

impl ModelCatalog {
    pub(crate) fn new(models: Vec<ModelPreset>) -> Self {
        Self {
            models,
            collaboration_modes: Vec::new(),
            collaboration_mode_overrides: HashMap::new(),
        }
    }

    pub(crate) fn with_collaboration_modes(mut self, modes: Vec<CollaborationModeMask>) -> Self {
        self.collaboration_modes = modes;
        self
    }

    pub(crate) fn with_collaboration_mode_overrides(
        mut self,
        overrides: HashMap<ModeKind, CollaborationModeOverride>,
    ) -> Self {
        self.collaboration_mode_overrides = overrides;
        self
    }

    pub(crate) fn collaboration_mode_overrides(
        &self,
    ) -> &HashMap<ModeKind, CollaborationModeOverride> {
        &self.collaboration_mode_overrides
    }

    pub(crate) fn try_list_models(&self) -> Result<Vec<ModelPreset>, Infallible> {
        Ok(self.models.clone())
    }

    pub(crate) fn display_name<'a>(&'a self, model: &'a str) -> &'a str {
        self.models
            .iter()
            .find(|preset| preset.model == model)
            .map(|preset| preset.display_name.as_str())
            .unwrap_or_else(|| model_display_name(model))
    }
}
