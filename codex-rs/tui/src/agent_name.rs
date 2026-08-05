#[cfg(not(test))]
use std::process::Command;

const AGENT_PERSONA_ENV: &str = "AGENT_PERSONA";

/// Resolve the pane-local agent name used by avatar and voice integrations.
///
/// Director owns the most specific identity when it is available. The
/// environment fallback keeps standalone and non-Director launches working.
pub(crate) fn resolve() -> Option<String> {
    let director_name = resolve_director_agent_name();
    let environment_name = std::env::var(AGENT_PERSONA_ENV).ok();
    resolve_from_sources(director_name.as_deref(), environment_name.as_deref())
}

#[cfg(not(test))]
fn resolve_director_agent_name() -> Option<String> {
    let output = Command::new("director")
        .args(["whoami", "--name"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
fn resolve_director_agent_name() -> Option<String> {
    None
}

fn resolve_from_sources(
    director_name: Option<&str>,
    environment_name: Option<&str>,
) -> Option<String> {
    director_name
        .and_then(normalize)
        .or_else(|| environment_name.and_then(normalize))
}

fn normalize(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn director_identity_wins_over_broader_environment_identity() {
        assert_eq!(
            resolve_from_sources(Some(" chloe\n"), Some("hai-os")),
            Some("chloe".to_string())
        );
    }

    #[test]
    fn environment_identity_is_used_without_director() {
        assert_eq!(
            resolve_from_sources(None, Some("centurion")),
            Some("centurion".to_string())
        );
    }

    #[test]
    fn invalid_names_are_ignored() {
        assert_eq!(
            resolve_from_sources(Some("not a name"), Some("also/bad")),
            None
        );
    }
}
