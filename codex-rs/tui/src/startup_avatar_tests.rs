use std::cell::Cell;

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn detected_identity_selects_its_bundled_avatar() {
    let home = tempfile::tempdir().unwrap();

    let binding = resolve_with(
        home.path(),
        /*explicit_name*/ None,
        || Some("chloe".to_string()),
    )
    .unwrap()
    .unwrap();

    assert_eq!(binding.character_id(), "chloe");
}

#[test]
fn explicit_name_wins_without_running_identity_detection() {
    let home = tempfile::tempdir().unwrap();
    let detector_called = Cell::new(false);

    let binding = resolve_with(home.path(), Some("centurion"), || {
        detector_called.set(true);
        Some("chloe".to_string())
    })
    .unwrap()
    .unwrap();

    assert_eq!(
        (binding.character_id(), detector_called.get()),
        ("centurion", false)
    );
}

#[test]
fn unknown_detected_identity_does_not_block_startup() {
    let home = tempfile::tempdir().unwrap();

    let binding = resolve_with(
        home.path(),
        /*explicit_name*/ None,
        || Some("unknown-agent".to_string()),
    )
    .unwrap();

    assert!(binding.is_none());
}
