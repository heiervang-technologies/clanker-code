use std::fs;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use pretty_assertions::assert_eq;

use super::TalkingSignal;
use super::flag_is_active;
use super::valid_agent_name;

#[test]
fn agent_names_are_safe_single_path_components() {
    assert_eq!(valid_agent_name("clanker-coder_1.0"), true);
    assert_eq!(valid_agent_name("../clanker"), false);
    assert_eq!(valid_agent_name("clanker coder"), false);
}

#[test]
fn fresh_flag_for_live_process_is_active_and_stale_flag_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clanker.json");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(
        &path,
        format!(r#"{{"started":{now},"pid":{}}}"#, std::process::id()),
    )
    .unwrap();
    assert_eq!(flag_is_active(&path), true);

    fs::write(
        &path,
        format!(
            r#"{{"started":{},"pid":{}}}"#,
            now.saturating_sub(121),
            std::process::id()
        ),
    )
    .unwrap();
    assert_eq!(flag_is_active(&path), false);
}

#[test]
fn missing_signal_is_cached_as_inactive() {
    let mut signal = TalkingSignal {
        path: Some(std::path::PathBuf::from("missing-talking-signal.json")),
        last_check: None,
        cached_active: true,
    };

    assert_eq!(signal.is_active(), false);
    assert_eq!(signal.is_active(), false);
}
