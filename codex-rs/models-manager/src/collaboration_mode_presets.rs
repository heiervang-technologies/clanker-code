use codex_collaboration_mode_templates::BASED as COLLABORATION_MODE_BASED;
use codex_collaboration_mode_templates::CRINGE as COLLABORATION_MODE_CRINGE;
use codex_collaboration_mode_templates::LOCKED_IN as COLLABORATION_MODE_LOCKED_IN;
use codex_collaboration_mode_templates::ULTRACHILL as COLLABORATION_MODE_ULTRACHILL;
use codex_prompts::ResolvedModelMessages;
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::TUI_VISIBLE_COLLABORATION_MODES;
use codex_protocol::openai_models::ReasoningEffort;

/// Built-in presets for every TUI-visible collaboration mode, in picker order.
pub fn builtin_collaboration_mode_presets() -> Vec<CollaborationModeMask> {
    TUI_VISIBLE_COLLABORATION_MODES
        .into_iter()
        .map(builtin_preset)
        .collect()
}

fn builtin_preset(mode: ModeKind) -> CollaborationModeMask {
    // Plan and LARP (the fork's default mode) resolve through the bundled
    // model messages so they stay aligned with upstream's "plan"/"default"
    // catalog slots; the remaining fork modes use their templates verbatim.
    let messages = ResolvedModelMessages::bundled().collaboration_modes();
    let (instructions, reasoning_effort) = match mode {
        ModeKind::Plan => (messages.plan.text(), Some(Some(ReasoningEffort::Medium))),
        ModeKind::Larp => (messages.default.text(), None),
        ModeKind::LockedIn => (COLLABORATION_MODE_LOCKED_IN, None),
        ModeKind::Based => (COLLABORATION_MODE_BASED, None),
        ModeKind::Cringe => (COLLABORATION_MODE_CRINGE, None),
        ModeKind::Ultrachill => (COLLABORATION_MODE_ULTRACHILL, None),
    };
    CollaborationModeMask {
        name: mode.display_name().to_string(),
        mode: Some(mode),
        model: None,
        reasoning_effort,
        developer_instructions: Some(Some(instructions.to_string())),
    }
}

#[cfg(test)]
#[path = "collaboration_mode_presets_tests.rs"]
mod tests;
