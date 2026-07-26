// Modified by Heiervang Technologies.
use super::*;
use pretty_assertions::assert_eq;

#[test]
fn presets_cover_all_tui_visible_modes_in_order() {
    let presets = builtin_collaboration_mode_presets();
    let modes: Vec<Option<ModeKind>> = presets.iter().map(|mask| mask.mode).collect();
    let expected: Vec<Option<ModeKind>> = TUI_VISIBLE_COLLABORATION_MODES
        .into_iter()
        .map(Some)
        .collect();
    assert_eq!(expected, modes);
}

#[test]
fn preset_names_use_mode_display_names() {
    for preset in builtin_collaboration_mode_presets() {
        let mode = preset.mode.expect("builtin preset should carry a mode");
        assert_eq!(preset.name, mode.display_name());
        assert_eq!(preset.model, None);
        let expected_effort = match mode {
            ModeKind::Plan => Some(Some(ReasoningEffort::Medium)),
            _ => None,
        };
        assert_eq!(expected_effort, preset.reasoning_effort);
    }
}

#[test]
fn mode_instructions_replace_mode_names_placeholder() {
    let known_mode_names = format_mode_names(&TUI_VISIBLE_COLLABORATION_MODES);
    let expected_snippet = format!("Known mode names are {known_mode_names}.");

    for preset in builtin_collaboration_mode_presets() {
        let mode = preset.mode.expect("builtin preset should carry a mode");
        let instructions = preset
            .developer_instructions
            .expect("builtin preset should include instructions")
            .expect("builtin instructions should be set");

        assert!(!instructions.contains("{{KNOWN_MODE_NAMES}}"));
        if mode != ModeKind::Plan {
            assert!(instructions.contains(&expected_snippet));
            assert!(instructions.contains(
                "Use the `request_user_input` tool only when it is listed in the available tools"
            ));
        }
    }
}

#[test]
fn larp_instructions_keep_plain_text_question_guidance() {
    let larp_instructions = builtin_collaboration_mode_presets()
        .into_iter()
        .find(|mask| mask.mode == Some(ModeKind::Larp))
        .expect("LARP preset should exist")
        .developer_instructions
        .expect("LARP preset should include instructions")
        .expect("LARP instructions should be set");

    assert!(larp_instructions.contains("ask the user directly with a concise plain-text question"));
}
