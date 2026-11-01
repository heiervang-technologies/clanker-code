use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::MemoryResetResponse;
use codex_app_server_protocol::RequestId;
use codex_character::CharacterCatalog;
use codex_features::Feature;
use codex_memories_write::memory_root_for_scope;
use codex_protocol::MemoryVersion;
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
use codex_utils_absolute_path::test_support::PathExt;
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
    MockResponsesConfig::new("http://127.0.0.1:9")
        .with_root_config("suppress_unstable_features_warning = true")
        .enable_feature(Feature::Sqlite)
        .write(codex_home.path())?;
    let state_db = init_state_db(codex_home.path()).await?;

    let mut thread_ids = Vec::new();
    for version in [MemoryVersion::V1, MemoryVersion::V2] {
        let root = codex_home.path().join(version.directory_name());
        tokio::fs::create_dir_all(root.join("rollout_summaries")).await?;
        tokio::fs::write(root.join("memory_summary.md"), "v1\nstale memory\n").await?;
        tokio::fs::write(
            root.join("rollout_summaries/stale.md"),
            "stale rollout summary\n",
        )
        .await?;
        thread_ids.push(seed_stage1_output(&state_db, codex_home.path(), version).await?);
    }

    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;

    let request_id = mcp
        .send_raw_request("memory/reset", /*params*/ None)
        .await?;
    let _: MemoryResetResponse =
        timeout(DEFAULT_READ_TIMEOUT, mcp.read_response(request_id)).await??;

    for version in [MemoryVersion::V1, MemoryVersion::V2] {
        let outputs = state_db
            .memories_for_version(version)
            .await?
            .list_stage1_outputs_for_global(/*n*/ 10)
            .await?;
        assert_eq!(outputs, Vec::new());
        let root = codex_home.path().join(version.directory_name());
        assert!(
            tokio::fs::read_dir(root)
                .await?
                .next_entry()
                .await?
                .is_none()
        );
    }
    for thread_id in thread_ids {
        assert_eq!(
            state_db.get_thread_memory_mode(thread_id).await?.as_deref(),
            Some("enabled")
        );
    }

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

pub(super) async fn seed_stage1_output(
    state_db: &Arc<StateRuntime>,
    codex_home: &Path,
    version: MemoryVersion,
) -> Result<ThreadId> {
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

    let store = state_db.memories_for_version(version).await?;
    let claim = store
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
        store
            .mark_stage1_job_succeeded(
                thread_id,
                ownership_token.as_str(),
                now.timestamp(),
                if version == MemoryVersion::V1 {
                    "raw memory"
                } else {
                    ""
                },
                "rollout summary",
                /*rollout_slug*/ None,
            )
            .await?,
        "stage1 success should be recorded"
    );
    store.enqueue_global_consolidation(now.timestamp()).await?;

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

pub(super) async fn init_state_db(codex_home: &Path) -> Result<Arc<StateRuntime>> {
    let state_db = StateRuntime::init(
        codex_state::SqliteConfig::new_for_testing(codex_home.abs()),
        "mock_provider".into(),
    )
    .await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;
    Ok(state_db)
}

#[tokio::test]
async fn memory_status_requires_successful_v2_consolidation_and_resets() -> Result<()> {
    use codex_app_server_protocol::MemoryStatusResponse;
    use codex_state::Phase2JobClaimOutcome;
    let home = TempDir::new()?;
    MockResponsesConfig::new("http://127.0.0.1:9")
        .enable_feature(Feature::Sqlite)
        .write(home.path())?;
    let db = init_state_db(home.path()).await?;
    let mut server = TestAppServer::builder()
        .with_codex_home(home.path())
        .without_auto_env()
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;
    let params = serde_json::json!({"minConsolidatedThreads": 2});
    // Resetting a fresh profile must not create an unused V1 workspace.
    let request = server
        .send_raw_request("memory/reset", /*params*/ None)
        .await?;
    let _: MemoryResetResponse = server.read_response(request).await?;
    assert!(!home.path().join("memories").exists());
    let request = server
        .send_raw_request("memory/status", Some(params.clone()))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 0,
            v2_ready: true
        }
    );
    // Even an empty V1 workspace retains the existing readiness requirements.
    tokio::fs::create_dir(home.path().join("memories")).await?;
    seed_stage1_output(&db, home.path(), MemoryVersion::V1).await?;
    let source = seed_stage1_output(&db, home.path(), MemoryVersion::V2).await?;
    seed_stage1_output(&db, home.path(), MemoryVersion::V2).await?;
    let request = server
        .send_raw_request("memory/status", Some(params.clone()))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 0,
            v2_ready: false
        }
    );

    let store = db.memories_for_version(MemoryVersion::V2).await?;
    let outputs = store.list_stage1_outputs_for_global(/*n*/ 20).await?;
    let Phase2JobClaimOutcome::Claimed {
        ownership_token,
        input_watermark,
    } = store
        .try_claim_global_phase2_job(source, /*lease_seconds*/ 60)
        .await?
    else {
        panic!("claim phase 2")
    };
    assert!(
        !store
            .mark_global_phase2_job_succeeded("wrong owner", input_watermark, &outputs)
            .await?
    );
    assert_eq!(store.max_consolidated_thread_count().await?, 0);
    assert!(
        store
            .mark_global_phase2_job_succeeded(&ownership_token, input_watermark, &outputs)
            .await?
    );
    // A completed job without a usable artifact must not activate v2.
    let request = server
        .send_raw_request("memory/status", Some(params.clone()))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 2,
            v2_ready: false
        }
    );
    let root = home.path().join("memories_v2");
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::write(root.join("memory_summary.md"), "v1\n## User Profile\nTest user\n## User preferences\nTest preference\n## General Tips\nTest tip\n## What's in Memory\nTest source\n").await?;
    db.delete_thread(source).await?;
    let request = server
        .send_raw_request("memory/status", Some(params.clone()))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 2,
            v2_ready: true
        }
    );
    // The default threshold remains 20 even though a caller can choose a smaller cohort.
    let request = server
        .send_raw_request("memory/status", Some(serde_json::json!({})))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 2,
            v2_ready: false
        }
    );
    let request = server
        .send_raw_request("memory/reset", /*params*/ None)
        .await?;
    let _: MemoryResetResponse = server.read_response(request).await?;
    let request = server
        .send_raw_request("memory/status", Some(params))
        .await?;
    let status: MemoryStatusResponse = server.read_response(request).await?;
    assert_eq!(
        status,
        MemoryStatusResponse {
            v2_consolidated_threads: 0,
            v2_ready: false
        }
    );
    Ok(())
}
