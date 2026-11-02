use std::path::Path;

use anyhow::Result;

use crate::avatars::AvatarBinding;

pub(crate) fn resolve(
    codex_home: &Path,
    explicit_name: Option<&str>,
) -> Result<Option<AvatarBinding>> {
    resolve_with(codex_home, explicit_name, crate::agent_name::resolve)
}

fn resolve_with<F>(
    codex_home: &Path,
    explicit_name: Option<&str>,
    detect_name: F,
) -> Result<Option<AvatarBinding>>
where
    F: FnOnce() -> Option<String>,
{
    let detected_name = explicit_name.is_none().then(detect_name).flatten();
    crate::avatars::resolve_startup_avatar_binding(
        codex_home,
        explicit_name,
        detected_name.as_deref(),
    )
}

#[cfg(test)]
#[path = "startup_avatar_tests.rs"]
mod tests;
