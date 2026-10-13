// Modified by Heiervang Technologies from the openai/codex original; see NOTICE for fork provenance.

use codex_protocol::config_types::CollaborationModeOverride;
use codex_protocol::config_types::ModeKind;
use codex_protocol::openai_models::ModelPreset;
use std::collections::HashMap;
use std::convert::Infallible;

#[derive(Debug, Clone)]
pub(crate) struct ModelCatalog {
    models: Vec<ModelPreset>,
    collaboration_mode_overrides: HashMap<ModeKind, CollaborationModeOverride>,
}

impl ModelCatalog {
    pub(crate) fn new(models: Vec<ModelPreset>) -> Self {
        Self {
            models,
            collaboration_mode_overrides: HashMap::new(),
        }
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
}
