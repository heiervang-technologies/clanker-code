// Modified by Heiervang Technologies.
use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::MemoryResetResponse;
use codex_app_server_protocol::RequestId;
use codex_character::CharacterCatalog;
use codex_memories_write::memory_root_for_scope;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use codex_state::CanonicalClankerId;
use codex_state::MemoryCitationPath;
use codex_state::MemoryProjectKey;
use codex_state::MemoryScope;
use codex_state::MemorySelectionScope;
use codex_state::Stage1JobClaimOutcome;
use codex_state::Stage1MemoryPayload;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::timeout;
use uuid::Uuid;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn memory_reset_clears_memory_files_and_rows_preserves_threads() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path())?;
    let state_db = init_state_db(codex_home.path()).await?;

    let memory_root = codex_home.path().join("memories");
    tokio::fs::create_dir_all(memory_root.join("rollout_summaries")).await?;
    tokio::fs::write(memory_root.join("MEMORY.md"), "stale memory\n").await?;
    tokio::fs::write(
        memory_root.join("rollout_summaries").join("stale.md"),
        "stale rollout summary\n",
    )
    .await?;

    let thread_id = seed_stage1_output(&state_db, codex_home.path()).await?;

    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_raw_request("memory/reset", /*params*/ None)
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let _: MemoryResetResponse = to_response::<MemoryResetResponse>(response)?;

    let stage1_outputs = state_db
        .memories()
        .list_stage1_outputs_for_global(/*n*/ 10)
        .await?;
    assert_eq!(stage1_outputs, Vec::new());
    assert_eq!(
        state_db.get_thread_memory_mode(thread_id).await?.as_deref(),
        Some("enabled")
    );

    let mut remaining_entries = tokio::fs::read_dir(&memory_root).await?;
    assert!(
        remaining_entries.next_entry().await?.is_none(),
        "memory root should be empty after reset"
    );

    Ok(())
}

#[tokio::test]
async fn thread_memory_reset_clears_only_receipt_scope() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path())?;
    let state_db = init_state_db(codex_home.path()).await?;
    let thread_id = seed_anonymous_stage1_output(&state_db, codex_home.path()).await?;
    let anonymous_file = codex_home.path().join("memories/MEMORY.md");
    let named_file = codex_home
        .path()
        .join("character_memories/characters/chloe/projects/project/MEMORY.md");
    tokio::fs::create_dir_all(anonymous_file.parent().unwrap()).await?;
    tokio::fs::create_dir_all(named_file.parent().unwrap()).await?;
    tokio::fs::write(&anonymous_file, "anonymous").await?;
    tokio::fs::write(&named_file, "named").await?;

    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let request_id = mcp
        .send_raw_request(
            "memory/reset",
            Some(json!({"scope": "thread", "threadId": thread_id.to_string()})),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let _: MemoryResetResponse = to_response(response)?;

    assert!(
        state_db
            .memories()
            .list_stage1_outputs_for_global(10)
            .await?
            .is_empty()
    );
    assert!(!tokio::fs::try_exists(&anonymous_file).await?);
    assert_eq!(tokio::fs::read_to_string(&named_file).await?, "named");
    Ok(())
}

#[tokio::test]
async fn character_memory_reset_rejects_anonymous_thread_scope() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path())?;
    let state_db = init_state_db(codex_home.path()).await?;
    let thread_id = seed_anonymous_stage1_output(&state_db, codex_home.path()).await?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_raw_request(
            "memory/reset",
            Some(json!({"scope": "character", "threadId": thread_id.to_string()})),
        )
        .await?;
    let error = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;

    assert_eq!(error.error.code, -32600);
    assert!(error.error.message.contains("is anonymous"));
    Ok(())
}

#[tokio::test]
async fn character_memory_reset_clears_only_that_character_across_projects() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path())?;
    write_character(codex_home.path(), "chloe")?;
    write_character(codex_home.path(), "clanker")?;
    let catalog = CharacterCatalog::load(codex_home.path());
    let chloe = CanonicalClankerId::resolve_exact(&catalog, "chloe")?;
    let clanker = CanonicalClankerId::resolve_exact(&catalog, "clanker")?;
    let project_a = MemoryProjectKey::from_git_origin("git@github.com:example/project-a.git")?;
    let project_b = MemoryProjectKey::from_git_origin("git@github.com:example/project-b.git")?;
    let state_db = init_state_db(codex_home.path()).await?;

    let chloe_a = seed_scoped_stage1_output(
        &state_db,
        codex_home.path(),
        Some(chloe.clone()),
        project_a.clone(),
        "chloe-a",
    )
    .await?;
    let chloe_b = seed_scoped_stage1_output(
        &state_db,
        codex_home.path(),
        Some(chloe.clone()),
        project_b.clone(),
        "chloe-b",
    )
    .await?;
    let clanker_a = seed_scoped_stage1_output(
        &state_db,
        codex_home.path(),
        Some(clanker.clone()),
        project_a.clone(),
        "clanker-a",
    )
    .await?;
    let anonymous = seed_scoped_stage1_output(
        &state_db,
        codex_home.path(),
        None,
        project_a.clone(),
        "anonymous",
    )
    .await?;

    let codex_home_absolute = AbsolutePathBuf::try_from(codex_home.path().to_path_buf())?;
    let chloe_a_scope = MemorySelectionScope::Named {
        clanker_id: chloe.clone(),
        project_key: project_a.clone(),
    };
    let chloe_b_scope = MemorySelectionScope::Named {
        clanker_id: chloe.clone(),
        project_key: project_b,
    };
    let clanker_scope = MemorySelectionScope::Named {
        clanker_id: clanker,
        project_key: project_a,
    };
    let chloe_a_file =
        memory_root_for_scope(&codex_home_absolute, &chloe_a_scope).join("MEMORY.md");
    let chloe_b_file =
        memory_root_for_scope(&codex_home_absolute, &chloe_b_scope).join("MEMORY.md");
    let clanker_file =
        memory_root_for_scope(&codex_home_absolute, &clanker_scope).join("MEMORY.md");
    let anonymous_file =
        memory_root_for_scope(&codex_home_absolute, &MemorySelectionScope::Anonymous)
            .join("MEMORY.md");
    for (path, contents) in [
        (&chloe_a_file, "chloe-a"),
        (&chloe_b_file, "chloe-b"),
        (&clanker_file, "clanker"),
        (&anonymous_file, "anonymous"),
    ] {
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::write(path, contents).await?;
    }

    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let request_id = mcp
        .send_raw_request(
            "memory/reset",
            Some(json!({"scope": "character", "threadId": chloe_a.to_string()})),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let _: MemoryResetResponse = to_response(response)?;

    assert!(
        state_db
            .memories()
            .select_scoped_memories(&chloe_a_scope, 10)
            .await?
            .is_empty()
    );
    assert!(
        state_db
            .memories()
            .select_scoped_memories(&chloe_b_scope, 10)
            .await?
            .is_empty()
    );
    assert_eq!(
        state_db
            .memories()
            .select_scoped_memories(&clanker_scope, 10)
            .await?
            .len(),
        1
    );
    assert_eq!(
        state_db
            .memories()
            .select_scoped_memories(&MemorySelectionScope::Anonymous, 10)
            .await?
            .len(),
        1
    );
    assert!(state_db.memories().memory_scope(chloe_a).await?.is_some());
    assert!(state_db.memories().memory_scope(chloe_b).await?.is_some());
    assert!(state_db.memories().memory_scope(clanker_a).await?.is_some());
    assert!(state_db.memories().memory_scope(anonymous).await?.is_some());
    assert!(!tokio::fs::try_exists(&chloe_a_file).await?);
    assert!(!tokio::fs::try_exists(&chloe_b_file).await?);
    assert_eq!(tokio::fs::read_to_string(&clanker_file).await?, "clanker");
    assert_eq!(
        tokio::fs::read_to_string(&anonymous_file).await?,
        "anonymous"
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn scoped_reset_commits_db_before_reporting_filesystem_failure() -> Result<()> {
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path())?;
    let state_db = init_state_db(codex_home.path()).await?;
    let thread_id = seed_anonymous_stage1_output(&state_db, codex_home.path()).await?;
    let outside = codex_home.path().join("outside");
    tokio::fs::create_dir_all(&outside).await?;
    let outside_file = outside.join("keep.txt");
    tokio::fs::write(&outside_file, "keep").await?;
    std::os::unix::fs::symlink(&outside, codex_home.path().join("memories"))?;

    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let request_id = mcp
        .send_raw_request(
            "memory/reset",
            Some(json!({"scope": "thread", "threadId": thread_id.to_string()})),
        )
        .await?;
    let error = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert!(
        error
            .error
            .message
            .contains("memory rows committed but filesystem cleanup")
    );
    assert!(
        state_db
            .memories()
            .list_stage1_outputs_for_global(10)
            .await?
            .is_empty(),
        "DB deletion must commit before filesystem cleanup"
    );
    assert_eq!(tokio::fs::read_to_string(outside_file).await?, "keep");
    Ok(())
}

async fn seed_stage1_output(state_db: &Arc<StateRuntime>, codex_home: &Path) -> Result<ThreadId> {
    let now = Utc::now();
    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string())?;
    let worker_id = ThreadId::from_string(&Uuid::new_v4().to_string())?;
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        codex_home.join("sessions").join("test.jsonl"),
        now,
        SessionSource::Cli,
    );
    builder.updated_at = Some(now);
    builder.cwd = codex_home.to_path_buf();
    let metadata = builder.build("mock_provider");
    state_db.upsert_thread(&metadata).await?;

    let claim = state_db
        .memories()
        .try_claim_stage1_job(
            thread_id,
            worker_id,
            now.timestamp(),
            /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await?;
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        anyhow::bail!("unexpected stage1 claim outcome: {claim:?}");
    };
    assert!(
        state_db
            .memories()
            .mark_stage1_job_succeeded(
                thread_id,
                ownership_token.as_str(),
                now.timestamp(),
                "raw memory",
                "rollout summary",
                /*rollout_slug*/ None,
            )
            .await?,
        "stage1 success should be recorded"
    );
    state_db
        .memories()
        .enqueue_global_consolidation(now.timestamp())
        .await?;

    Ok(thread_id)
}

async fn seed_anonymous_stage1_output(
    state_db: &Arc<StateRuntime>,
    codex_home: &Path,
) -> Result<ThreadId> {
    seed_scoped_stage1_output(
        state_db,
        codex_home,
        None,
        MemoryProjectKey::from_git_origin("git@github.com:example/anonymous.git")?,
        "anonymous",
    )
    .await
}

async fn seed_scoped_stage1_output(
    state_db: &Arc<StateRuntime>,
    codex_home: &Path,
    clanker_id: Option<CanonicalClankerId>,
    project_key: MemoryProjectKey,
    label: &str,
) -> Result<ThreadId> {
    let now = Utc::now();
    let thread_id = ThreadId::new();
    let worker_id = ThreadId::new();
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        codex_home
            .join("sessions")
            .join(format!("{thread_id}.jsonl")),
        now,
        SessionSource::Cli,
    );
    builder.updated_at = Some(now);
    builder.cwd = codex_home.to_path_buf();
    state_db
        .upsert_thread(&builder.build("mock_provider"))
        .await?;
    let claim = state_db
        .memories()
        .try_claim_stage1_job(thread_id, worker_id, now.timestamp(), 3_600, 64)
        .await?;
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        anyhow::bail!("unexpected scoped stage1 claim: {claim:?}");
    };
    let scope = MemoryScope {
        thread_id,
        clanker_id,
        project_key,
        parent_thread_id: None,
        recorded_at: now,
    };
    let citation = MemoryCitationPath::new(format!("rollout_summaries/{thread_id}.md"))?;
    assert!(
        state_db
            .memories()
            .mark_stage1_job_succeeded_scoped(
                &scope,
                &citation,
                ownership_token.as_str(),
                Stage1MemoryPayload {
                    source_updated_at: now.timestamp(),
                    raw_memory: label,
                    rollout_summary: label,
                    rollout_slug: None,
                },
            )
            .await?
    );
    Ok(thread_id)
}

fn write_character(codex_home: &Path, id: &str) -> Result<()> {
    let package = codex_home.join("characters").join(id);
    let avatar = package.join("avatar");
    std::fs::create_dir_all(&avatar)?;
    std::fs::write(
        package.join("character.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "id": id,
            "displayName": id,
            "avatar": "avatar/avatar.json"
        }))?,
    )?;
    std::fs::write(
        avatar.join("avatar.json"),
        r#"{
            "renderMode": "ansi-half-block",
            "spritesheetPath": "sheet.ppm",
            "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1}
        }"#,
    )?;
    let pixels = "0 0 0\n".repeat(24 * 24);
    std::fs::write(
        avatar.join("sheet.ppm"),
        format!("P3\n24 24\n255\n{pixels}"),
    )?;
    Ok(())
}

async fn init_state_db(codex_home: &Path) -> Result<Arc<StateRuntime>> {
    let state_db = StateRuntime::init(codex_home.to_path_buf(), "mock_provider".into()).await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;
    Ok(state_db)
}

fn create_config_toml(codex_home: &Path) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
model_provider = "mock_provider"
suppress_unstable_features_warning = true

[features]
sqlite = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "http://127.0.0.1:9/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
    )
}
