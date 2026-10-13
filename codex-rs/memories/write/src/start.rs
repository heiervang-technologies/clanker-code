// Modified by Heiervang Technologies from the openai/codex original; see NOTICE for fork provenance.

use crate::extensions::seed_extension_instructions;
use crate::guard;
use crate::metrics::MEMORY_STARTUP;
use crate::phase1;
use crate::phase2;
use crate::runtime::MemoryStartupContext;
use crate::scope;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_features::Feature;
use codex_login::AuthManager;
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::SessionSource;
use codex_state::MemoryScope;
use std::sync::Arc;
use tracing::warn;

/// Prepares the immutable scope consumed by startup context and memory generation.
///
/// Explicit named launches fail closed before their first turn. Anonymous
/// compatibility launches keep working when optional memory infrastructure is
/// unavailable.
pub async fn prepare_memories_startup_scope(
    thread_id: ThreadId,
    thread: &CodexThread,
    config: &Config,
    source: &SessionSource,
) -> anyhow::Result<Option<MemoryScope>> {
    let launcher_identity = std::env::var_os("CLANKER_ID");
    prepare_memories_startup_scope_with_state_db(
        thread_id,
        thread,
        config,
        source,
        thread.state_db(),
        launcher_identity,
    )
    .await
}

pub(crate) async fn prepare_memories_startup_scope_with_state_db(
    thread_id: ThreadId,
    thread: &CodexThread,
    config: &Config,
    source: &SessionSource,
    state_db: Option<Arc<codex_state::StateRuntime>>,
    launcher_identity: Option<std::ffi::OsString>,
) -> anyhow::Result<Option<MemoryScope>> {
    if config.ephemeral
        || !config.features.enabled(Feature::MemoryTool)
        || source.is_non_root_agent()
    {
        return Ok(None);
    }

    let explicit_identity = launcher_identity.is_some();
    let Some(db) = state_db else {
        if explicit_identity {
            anyhow::bail!("state db unavailable for explicit character memory scope");
        }
        warn!("state db unavailable for memories startup pipeline; skipping");
        return Ok(None);
    };
    let config_snapshot = thread.config_snapshot().await;
    let parent_thread_id = config_snapshot
        .parent_thread_id
        .or(config_snapshot.forked_from_thread_id);
    match scope::current_thread_scope(
        db.as_ref(),
        thread_id,
        &config.codex_home,
        &config.cwd,
        parent_thread_id,
        launcher_identity,
    )
    .await
    {
        Ok(scope) => Ok(Some(scope)),
        Err(err) if explicit_identity => Err(err),
        Err(err) => {
            warn!("failed preparing anonymous memory scope; preserving legacy turn: {err}");
            Ok(None)
        }
    }
}

/// Starts the asynchronous startup memory pipeline for an eligible root session.
///
/// The pipeline is skipped for ephemeral sessions, disabled feature flags, and
/// subagent sessions.
pub fn start_memories_startup_task(
    thread_manager: Arc<ThreadManager>,
    auth_manager: Arc<AuthManager>,
    thread: Arc<CodexThread>,
    config: Arc<Config>,
    parent_permission_profile: PermissionProfile,
    source: &SessionSource,
    memory_scope: MemoryScope,
) {
    if config.ephemeral
        || !config.features.enabled(Feature::MemoryTool)
        || source.is_non_root_agent()
    {
        return;
    }

    let thread_id = memory_scope.thread_id;
    let context = Arc::new(MemoryStartupContext::new(
        thread_manager,
        Arc::clone(&auth_manager),
        thread_id,
        thread,
        config.as_ref(),
        source.clone(),
    ));

    tokio::spawn(async move {
        let selection_scope = memory_scope.selection_scope();
        let root = crate::memory_root_for_scope(&config.codex_home, &selection_scope);
        if let Err(err) = tokio::fs::create_dir_all(&root).await {
            warn!("failed creating memories root: {err}");
            return;
        }
        if let Err(err) = seed_extension_instructions(&root).await {
            warn!("failed seeding memory extension instructions: {err}");
        }

        // Clean memories to make preserve DB size. This does not consume tokens so can be
        // done before the quota check.
        phase1::prune(context.as_ref(), &config).await;

        if !guard::rate_limits_ok(&auth_manager, &config).await {
            context.counter(
                MEMORY_STARTUP,
                /*inc*/ 1,
                &[("status", "skipped_rate_limit")],
            );
            return;
        }

        // Run phase 1.
        phase1::run(Arc::clone(&context), Arc::clone(&config)).await;
        // Run phase 2.
        phase2::run(context, config, parent_permission_profile, selection_scope).await;
    });
}
