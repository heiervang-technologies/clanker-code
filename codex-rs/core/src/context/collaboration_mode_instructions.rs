// Modified by Heiervang Technologies.
use super::ContextualUserFragment;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::protocol::COLLABORATION_MODE_CLOSE_TAG;
use codex_protocol::protocol::COLLABORATION_MODE_OPEN_TAG;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CollaborationModeInstructions {
    instructions: String,
}

impl CollaborationModeInstructions {
    pub(crate) fn from_collaboration_mode(collaboration_mode: &CollaborationMode) -> Option<Self> {
        collaboration_mode
            .settings
            .developer_instructions
            .as_ref()
            .filter(|instructions| !instructions.is_empty())
            .map(|instructions| Self {
                instructions: instructions.clone(),
            })
    }

    /// One-line switch notice used when this mode's full instructions are
    /// already present in the conversation history, so re-sending the full
    /// document would only bloat context.
    pub(crate) fn switch_reminder(collaboration_mode: &CollaborationMode) -> Option<Self> {
        collaboration_mode
            .settings
            .developer_instructions
            .as_ref()
            .filter(|instructions| !instructions.is_empty())?;
        let mode_name = collaboration_mode.mode.display_name();
        Some(Self {
            instructions: format!(
                "You are now in {mode_name} mode. Your earlier {mode_name} mode instructions in this conversation apply again; any previous instructions for other modes are no longer active."
            ),
        })
    }
}

impl ContextualUserFragment for CollaborationModeInstructions {
    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (COLLABORATION_MODE_OPEN_TAG, COLLABORATION_MODE_CLOSE_TAG)
    }

    fn body(&self) -> String {
        self.instructions.clone()
    }
}
