use std::process::Command;

const AGENT_PERSONA_ENV: &str = "AGENT_PERSONA";

/// Resolve the pane-local agent name used by avatar and voice integrations.
///
/// The process-scoped environment identity takes precedence so avatar and voice
/// integrations observe the same agent. Director remains the standalone
/// fallback when that environment identity is unavailable or invalid.
pub(crate) fn resolve() -> Option<String> {
    let environment_name = std::env::var(AGENT_PERSONA_ENV).ok();
    resolve_from_sources(environment_name.as_deref(), resolve_director_agent_name)
}

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

fn resolve_from_sources<F>(environment_name: Option<&str>, director_name: F) -> Option<String>
where
    F: FnOnce() -> Option<String>,
{
    environment_name
        .and_then(normalize)
        .or_else(|| director_name().as_deref().and_then(normalize))
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
#[path = "agent_name_tests.rs"]
mod tests;
