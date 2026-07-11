//! Builds the `/pets` picker dialog for the TUI.
//!
//! The picker deliberately merges three sources into one list:
//! built-in catalog pets, a synthetic "disable" entry, and user-managed custom
//! pets. It does not load preview images itself; instead it emits selection
//! change events so the surrounding chat widget can coordinate async asset
//! downloads, preview loading, and final config persistence.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ratatui::style::Stylize;
use ratatui::text::Line;

use crate::app_event::AppEvent;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionTab;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::SideContentWidth;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;

use super::DEFAULT_PET_ID;
use super::DISABLED_PET_ID;
use super::catalog;
use super::model::CUSTOM_PET_PREFIX;
use super::model::Pet;
use super::model::custom_pet_selector;
use super::preview::PetPickerPreviewState;

pub(crate) const PET_PICKER_VIEW_ID: &str = "pet-picker";
const PET_PICKER_PREVIEW_WIDTH: u16 = 30;
const PET_PREVIEW_STATES: &[(&str, &str)] = &[
    ("idle", "Idle"),
    ("running", "R"),
    ("talking", "T"),
    ("waiting", "W"),
    ("review", "V"),
    ("failed", "F"),
    ("planning", "P"),
    ("tired-idle", "Ti"),
    ("tired-running", "Tr"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct PetPickerEntry {
    selector: String,
    legacy_selector: Option<String>,
    display_name: String,
    description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetCycleDirection {
    Next,
    Previous,
}

pub(crate) fn adjacent_pet_selector(
    current_pet: Option<&str>,
    codex_home: &Path,
    direction: PetCycleDirection,
) -> Option<String> {
    let mut entries = available_pet_entries(codex_home);
    entries.retain(|entry| entry.selector != DISABLED_PET_ID);
    entries.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.selector.cmp(&right.selector))
    });
    let current_idx = current_pet.and_then(|current_pet| {
        entries.iter().position(|entry| {
            entry.selector == current_pet || entry.legacy_selector.as_deref() == Some(current_pet)
        })
    });
    let next_idx = match (current_idx, direction) {
        (Some(idx), PetCycleDirection::Next) => (idx + 1) % entries.len(),
        (Some(0), PetCycleDirection::Previous) | (None, PetCycleDirection::Previous) => {
            entries.len().checked_sub(1)?
        }
        (Some(idx), PetCycleDirection::Previous) => idx - 1,
        (None, PetCycleDirection::Next) => 0,
    };
    entries.get(next_idx).map(|entry| entry.selector.clone())
}

/// Build the selection popup parameters for `/pets`.
///
/// The picker preselects `DEFAULT_PET_ID` when no pet is configured so the UI
/// has a sensible starting point without implying that Codex is already the
/// active ambient pet. Callers should treat the returned actions as the only
/// supported mutation path; bypassing them would skip preview-loading and
/// selection-specific event wiring.
pub(crate) fn build_pet_picker_params(
    current_pet: Option<&str>,
    codex_home: &Path,
    preview_state: PetPickerPreviewState,
) -> SelectionViewParams {
    let preferred_pet = current_pet.unwrap_or(DEFAULT_PET_ID);
    let mut entries = available_pet_entries(codex_home);
    entries.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.selector.cmp(&right.selector))
    });
    if let Some(disabled_idx) = entries
        .iter()
        .position(|entry| entry.selector == DISABLED_PET_ID)
    {
        let disabled_entry = entries.remove(disabled_idx);
        entries.insert(0, disabled_entry);
    }

    let preview_pet_ids = entries
        .iter()
        .map(|entry| entry.selector.clone())
        .collect::<Vec<_>>();
    let on_selection_changed: crate::bottom_pane::OnSelectionChangedCallback = Some(Box::new(
        move |idx: usize, tx: &crate::app_event_sender::AppEventSender| {
            if let Some(pet_id) = preview_pet_ids.get(idx) {
                tx.send(AppEvent::PetPreviewRequested {
                    pet_id: pet_id.clone(),
                });
            }
        },
    ));

    let initial_selected_idx = entries.iter().position(|entry| {
        preferred_pet == entry.selector || entry.legacy_selector.as_deref() == Some(preferred_pet)
    });
    let tabs = PET_PREVIEW_STATES
        .iter()
        .map(|(animation_name, label)| SelectionTab {
            id: (*animation_name).to_string(),
            label: (*label).to_string(),
            header: wheel_header(animation_name),
            items: pet_selection_items(&entries, current_pet),
        })
        .collect();
    let on_tab_changed: crate::bottom_pane::OnTabChangedCallback = Some(Box::new(
        |animation_name: &str, tx: &crate::app_event_sender::AppEventSender| {
            tx.send(AppEvent::PetPreviewStateChanged {
                animation_name: animation_name.to_string(),
            });
        },
    ));

    SelectionViewParams {
        view_id: Some(PET_PICKER_VIEW_ID),
        footer_hint: Some("↑/↓ avatar · ←/→ state · enter select · esc cancel".into()),
        tabs,
        initial_tab_id: Some("idle".to_string()),
        is_searchable: true,
        search_placeholder: Some("Type to filter avatars...".to_string()),
        initial_selected_idx,
        side_content: Box::new(preview_state.renderable()),
        side_content_width: SideContentWidth::Fixed(PET_PICKER_PREVIEW_WIDTH),
        side_content_min_width: 28,
        stacked_side_content: Some(Box::new(())),
        preserve_side_content_bg: true,
        on_selection_changed,
        on_tab_changed,
        ..Default::default()
    }
}

fn wheel_header(animation_name: &str) -> Box<dyn Renderable> {
    let mut header = ColumnRenderable::new();
    header.push(Line::from("Avatar Wheel".bold()));
    header.push(Line::from(format!("Previewing: {animation_name}").dim()));
    Box::new(header)
}

fn pet_selection_items(
    entries: &[PetPickerEntry],
    current_pet: Option<&str>,
) -> Vec<SelectionItem> {
    entries
        .iter()
        .map(|entry| {
            let is_current = current_pet.is_some_and(|current_pet| {
                current_pet == entry.selector
                    || entry.legacy_selector.as_deref() == Some(current_pet)
            });
            let pet_id = entry.selector.clone();
            let search_value = if pet_id == DISABLED_PET_ID {
                "disable disabled hide hidden off none".to_string()
            } else {
                entry.selector.clone()
            };
            let actions: Vec<SelectionAction> = if pet_id == DISABLED_PET_ID {
                vec![Box::new(|tx| {
                    tx.send(AppEvent::PetDisabled);
                })]
            } else {
                vec![Box::new(move |tx| {
                    tx.send(AppEvent::PetSelected {
                        pet_id: pet_id.clone(),
                    });
                })]
            };
            SelectionItem {
                name: entry.display_name.clone(),
                description: entry.description.clone(),
                is_current,
                dismiss_on_select: true,
                search_value: Some(search_value),
                actions,
                ..Default::default()
            }
        })
        .collect()
}

fn available_pet_entries(codex_home: &Path) -> Vec<PetPickerEntry> {
    let mut entries = catalog::BUILTIN_PETS
        .iter()
        .map(|pet| PetPickerEntry {
            selector: pet.id.to_string(),
            legacy_selector: None,
            display_name: pet.display_name.to_string(),
            description: Some(pet.description.to_string()),
        })
        .collect::<Vec<_>>();
    entries.push(PetPickerEntry {
        selector: DISABLED_PET_ID.to_string(),
        legacy_selector: None,
        display_name: "Disable terminal pets".to_string(),
        description: None,
    });
    entries.extend(custom_pet_entries(codex_home));
    entries
}

fn custom_pet_entries(codex_home: &Path) -> Vec<PetPickerEntry> {
    let mut entries_by_selector = HashMap::new();
    for (directory_name, manifest_file) in [("avatars", "avatar.json"), ("pets", "pet.json")] {
        let Ok(children) = fs::read_dir(codex_home.join(directory_name)) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            if !path.join(manifest_file).is_file() {
                continue;
            }
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if id == DISABLED_PET_ID || id.starts_with(CUSTOM_PET_PREFIX) {
                continue;
            }
            let selector = custom_pet_selector(id);
            let Ok(pet) =
                Pet::load_with_codex_home(&selector, /*codex_home*/ Some(codex_home))
            else {
                continue;
            };
            entries_by_selector.insert(
                selector.clone(),
                PetPickerEntry {
                    selector,
                    legacy_selector: Some(id.to_string()),
                    display_name: pet.display_name,
                    description: (!pet.description.is_empty()).then_some(pet.description),
                },
            );
        }
    }

    entries_by_selector.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle_items(params: &SelectionViewParams) -> &[SelectionItem] {
        &params.tabs[0].items
    }

    fn write_pet(dir: &Path, folder_name: &str, display_name: &str) {
        let pet_dir = dir.join("pets").join(folder_name);
        fs::create_dir_all(&pet_dir).unwrap();
        fs::write(
            pet_dir.join("pet.json"),
            format!(
                r#"{{
                    "id": "{folder_name}",
                    "displayName": "{display_name}",
                    "description": "custom pet",
                    "spritesheetPath": "spritesheet.webp"
                }}"#
            ),
        )
        .unwrap();
        catalog::write_test_spritesheet(&pet_dir.join("spritesheet.webp"));
    }

    fn write_legacy_avatar(dir: &Path, folder_name: &str, display_name: &str) {
        let avatar_dir = dir.join("avatars").join(folder_name);
        fs::create_dir_all(&avatar_dir).unwrap();
        fs::write(
            avatar_dir.join("avatar.json"),
            format!(
                r#"{{
                    "displayName": "{display_name}",
                    "description": "legacy custom pet",
                    "spritesheetPath": "spritesheet.webp"
                }}"#
            ),
        )
        .unwrap();
        catalog::write_test_spritesheet(&avatar_dir.join("spritesheet.webp"));
    }

    #[test]
    fn picker_lists_app_bundled_and_custom_pets() {
        let codex_home = tempfile::tempdir().unwrap();
        write_pet(codex_home.path(), "chefito", "Chefito");

        let params = build_pet_picker_params(
            Some("chefito"),
            codex_home.path(),
            PetPickerPreviewState::default(),
        );

        assert_eq!(
            idle_items(&params)
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Disable terminal pets",
                "BSOD",
                "Chefito",
                "Codex",
                "Dewey",
                "Fireball",
                "Null Signal",
                "Rocky",
                "Seedy",
                "Stacky",
            ],
        );
        assert_eq!(params.initial_selected_idx, Some(2));
        assert_eq!(
            idle_items(&params)[2].search_value.as_deref(),
            Some("custom:chefito")
        );
        assert_eq!(
            params
                .tabs
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "idle",
                "running",
                "talking",
                "waiting",
                "review",
                "failed",
                "planning",
                "tired-idle",
                "tired-running",
            ]
        );
    }

    #[test]
    fn adjacent_pet_selector_cycles_stable_picker_order_and_legacy_ids() {
        let codex_home = tempfile::tempdir().unwrap();
        write_pet(codex_home.path(), "chefito", "Chefito");

        assert_eq!(
            adjacent_pet_selector(Some("chefito"), codex_home.path(), PetCycleDirection::Next,),
            Some("codex".to_string())
        );
        assert_eq!(
            adjacent_pet_selector(
                Some("custom:chefito"),
                codex_home.path(),
                PetCycleDirection::Previous,
            ),
            Some("bsod".to_string())
        );
        assert_eq!(
            adjacent_pet_selector(Some("bsod"), codex_home.path(), PetCycleDirection::Previous,),
            Some("stacky".to_string())
        );
    }

    #[test]
    fn picker_preselects_codex_without_marking_it_current_when_no_pet_is_configured() {
        let codex_home = tempfile::tempdir().unwrap();
        let params = build_pet_picker_params(
            /*current_pet*/ None,
            codex_home.path(),
            PetPickerPreviewState::default(),
        );

        assert_eq!(params.initial_selected_idx, Some(2));
        assert_eq!(idle_items(&params)[2].name, "Codex");
        assert!(!idle_items(&params)[2].is_current);
    }

    #[test]
    fn picker_marks_disabled_pet_as_current() {
        let codex_home = tempfile::tempdir().unwrap();
        let params = build_pet_picker_params(
            Some(DISABLED_PET_ID),
            codex_home.path(),
            PetPickerPreviewState::default(),
        );

        assert_eq!(params.initial_selected_idx, Some(0));
        assert_eq!(idle_items(&params)[0].name, "Disable terminal pets");
        assert_eq!(idle_items(&params)[0].description, None);
        assert!(idle_items(&params)[0].is_current);
        assert_eq!(
            idle_items(&params)[0].search_value.as_deref(),
            Some("disable disabled hide hidden off none")
        );
    }

    #[test]
    fn picker_imports_legacy_avatar_manifests() {
        let codex_home = tempfile::tempdir().unwrap();
        write_legacy_avatar(codex_home.path(), "legacy", "Legacy");

        let params = build_pet_picker_params(
            Some("custom:legacy"),
            codex_home.path(),
            PetPickerPreviewState::default(),
        );
        let legacy = params.tabs[0]
            .items
            .iter()
            .find(|item| item.name == "Legacy")
            .unwrap();

        assert!(legacy.is_current);
        assert_eq!(legacy.search_value.as_deref(), Some("custom:legacy"));
    }
}
