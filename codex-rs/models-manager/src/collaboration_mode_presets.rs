use codex_collaboration_mode_templates::BASED as COLLABORATION_MODE_BASED;
use codex_collaboration_mode_templates::CRINGE as COLLABORATION_MODE_CRINGE;
use codex_collaboration_mode_templates::LARP as COLLABORATION_MODE_LARP;
use codex_collaboration_mode_templates::LOCKED_IN as COLLABORATION_MODE_LOCKED_IN;
use codex_collaboration_mode_templates::PLAN as COLLABORATION_MODE_PLAN;
use codex_collaboration_mode_templates::ULTRACHILL as COLLABORATION_MODE_ULTRACHILL;
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::TUI_VISIBLE_COLLABORATION_MODES;
use codex_protocol::openai_models::ReasoningEffort;
use codex_utils_template::Template;

const KNOWN_MODE_NAMES_TEMPLATE_KEY: &str = "KNOWN_MODE_NAMES";

/// Built-in presets for every TUI-visible collaboration mode, in picker order.
pub fn builtin_collaboration_mode_presets() -> Vec<CollaborationModeMask> {
    TUI_VISIBLE_COLLABORATION_MODES
        .into_iter()
        .map(builtin_preset)
        .collect()
}

fn builtin_preset(mode: ModeKind) -> CollaborationModeMask {
    // Plan's template has no placeholders and the template engine rejects
    // unused values, so it is used verbatim.
    let (instructions, reasoning_effort) = match mode {
        ModeKind::Plan => (
            COLLABORATION_MODE_PLAN.to_string(),
            Some(Some(ReasoningEffort::Medium)),
        ),
        ModeKind::Larp => (
            render_mode_instructions(mode, COLLABORATION_MODE_LARP),
            None,
        ),
        ModeKind::LockedIn => (
            render_mode_instructions(mode, COLLABORATION_MODE_LOCKED_IN),
            None,
        ),
        ModeKind::Based => (
            render_mode_instructions(mode, COLLABORATION_MODE_BASED),
            None,
        ),
        ModeKind::Cringe => (
            render_mode_instructions(mode, COLLABORATION_MODE_CRINGE),
            None,
        ),
        ModeKind::Ultrachill => (
            render_mode_instructions(mode, COLLABORATION_MODE_ULTRACHILL),
            None,
        ),
        ModeKind::PairProgramming | ModeKind::Execute => {
            unreachable!("hidden modes have no built-in preset")
        }
    };
    CollaborationModeMask {
        name: mode.display_name().to_string(),
        mode: Some(mode),
        model: None,
        reasoning_effort,
        developer_instructions: Some(Some(instructions)),
    }
}

fn render_mode_instructions(mode: ModeKind, template: &str) -> String {
    let known_mode_names = format_mode_names(&TUI_VISIBLE_COLLABORATION_MODES);
    Template::parse(template)
        .unwrap_or_else(|err| panic!("{mode:?} collaboration mode template must parse: {err}"))
        .render([(KNOWN_MODE_NAMES_TEMPLATE_KEY, known_mode_names.as_str())])
        .unwrap_or_else(|err| panic!("{mode:?} collaboration mode template must render: {err}"))
}

fn format_mode_names(modes: &[ModeKind]) -> String {
    let mode_names: Vec<&str> = modes.iter().map(|mode| mode.display_name()).collect();
    match mode_names.as_slice() {
        [] => "none".to_string(),
        [mode_name] => (*mode_name).to_string(),
        [first, second] => format!("{first} and {second}"),
        [..] => mode_names.join(", "),
    }
}

#[cfg(test)]
#[path = "collaboration_mode_presets_tests.rs"]
mod tests;
