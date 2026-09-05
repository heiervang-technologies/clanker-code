use std::cell::Cell;

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn environment_identity_wins_without_querying_director() {
    let director_called = Cell::new(false);

    let resolved = resolve_from_sources(Some(" hai-os\n"), || {
        director_called.set(true);
        Some("chloe".to_string())
    });

    assert_eq!(
        (resolved, director_called.get()),
        (Some("hai-os".to_string()), false)
    );
}

#[test]
fn director_identity_is_used_when_environment_identity_is_unavailable() {
    assert_eq!(
        resolve_from_sources(None, || Some(" centurion\n".to_string())),
        Some("centurion".to_string())
    );
    assert_eq!(
        resolve_from_sources(Some("not a name"), || Some("chloe".to_string())),
        Some("chloe".to_string())
    );
}

#[test]
fn invalid_names_are_ignored() {
    assert_eq!(
        resolve_from_sources(Some("not a name"), || Some("also/bad".to_string())),
        None
    );
}
