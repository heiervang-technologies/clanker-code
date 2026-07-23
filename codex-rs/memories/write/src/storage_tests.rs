use super::rollout_summary_file_stem;
use crate::ensure_layout;
use crate::raw_memories_file;
use crate::rebuild_raw_memories_file_from_memories;
use crate::rebuild_raw_memories_file_from_scoped_memories;
use crate::rollout_summaries_dir;
use crate::sync_rollout_summaries_from_memories;
use crate::sync_rollout_summaries_from_scoped_memories;
use chrono::TimeZone;
use chrono::Utc;
use codex_config::types::DEFAULT_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION;
use codex_protocol::ThreadId;
use codex_state::CanonicalClankerId;
use codex_state::MemoryCitationPath;
use codex_state::MemoryProjectKey;
use codex_state::MemorySelectionScope;
use codex_state::MemoryVisibility;
use codex_state::ScopedMemoryRecord;
use codex_state::Stage1Output;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use tempfile::tempdir;

const FIXED_PREFIX: &str = "2025-02-11T15-35-19-jqmb";

fn stage1_output_with_slug(thread_id: ThreadId, rollout_slug: Option<&str>) -> Stage1Output {
    Stage1Output {
        thread_id,
        source_updated_at: Utc.timestamp_opt(123, 0).single().expect("timestamp"),
        raw_memory: "raw memory".to_string(),
        rollout_summary: "summary".to_string(),
        rollout_slug: rollout_slug.map(ToString::to_string),
        rollout_path: PathBuf::from("/tmp/rollout.jsonl"),
        cwd: PathBuf::from("/tmp/workspace"),
        git_branch: None,
        generated_at: Utc.timestamp_opt(124, 0).single().expect("timestamp"),
    }
}

fn fixed_thread_id() -> ThreadId {
    ThreadId::try_from("0194f5a6-89ab-7cde-8123-456789abcdef").expect("valid thread id")
}

#[test]
fn rollout_summary_file_stem_uses_uuid_timestamp_and_hash_when_slug_missing() {
    let thread_id = fixed_thread_id();
    let memory = stage1_output_with_slug(thread_id, /*rollout_slug*/ None);

    assert_eq!(rollout_summary_file_stem(&memory), FIXED_PREFIX);
}

#[test]
fn rollout_summary_file_stem_sanitizes_and_truncates_slug() {
    let thread_id = fixed_thread_id();
    let memory = stage1_output_with_slug(
        thread_id,
        Some("Unsafe Slug/With Spaces & Symbols + EXTRA_LONG_12345_67890_ABCDE_fghij_klmno"),
    );

    let stem = rollout_summary_file_stem(&memory);
    let slug = stem
        .strip_prefix(&format!("{FIXED_PREFIX}-"))
        .expect("slug suffix should be present");
    assert_eq!(slug.len(), 60);
    assert_eq!(
        slug,
        "unsafe_slug_with_spaces___symbols___extra_long_12345_67890_a"
    );
}

#[test]
fn rollout_summary_file_stem_uses_uuid_timestamp_and_hash_when_slug_is_empty() {
    let thread_id = fixed_thread_id();
    let memory = stage1_output_with_slug(thread_id, Some(""));

    assert_eq!(rollout_summary_file_stem(&memory), FIXED_PREFIX);
}

#[tokio::test]
async fn sync_rollout_summaries_and_raw_memories_file_keeps_latest_memories_only() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("memory");
    ensure_layout(&root).await.expect("ensure layout");

    let keep_id = ThreadId::default().to_string();
    let drop_id = ThreadId::default().to_string();
    let keep_path = rollout_summaries_dir(&root).join(format!("{keep_id}.md"));
    let drop_path = rollout_summaries_dir(&root).join(format!("{drop_id}.md"));
    tokio::fs::write(&keep_path, "keep")
        .await
        .expect("write keep");
    tokio::fs::write(&drop_path, "drop")
        .await
        .expect("write drop");

    let memories = vec![Stage1Output {
        thread_id: ThreadId::try_from(keep_id.clone()).expect("thread id"),
        source_updated_at: Utc.timestamp_opt(100, 0).single().expect("timestamp"),
        raw_memory: "raw memory".to_string(),
        rollout_summary: "short summary".to_string(),
        rollout_slug: None,
        rollout_path: PathBuf::from("/tmp/rollout-100.jsonl"),
        cwd: PathBuf::from("/tmp/workspace"),
        git_branch: None,
        generated_at: Utc.timestamp_opt(101, 0).single().expect("timestamp"),
    }];

    sync_rollout_summaries_from_memories(
        &root,
        &memories,
        DEFAULT_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION,
    )
    .await
    .expect("sync rollout summaries");
    rebuild_raw_memories_file_from_memories(
        &root,
        &memories,
        DEFAULT_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION,
    )
    .await
    .expect("rebuild raw memories");

    assert!(
        !tokio::fs::try_exists(&keep_path)
            .await
            .expect("check stale keep path"),
        "sync should prune stale filename that used thread id only"
    );
    assert!(
        !tokio::fs::try_exists(&drop_path)
            .await
            .expect("check stale drop path"),
        "sync should prune stale filename for dropped thread"
    );

    let mut dir = tokio::fs::read_dir(rollout_summaries_dir(&root))
        .await
        .expect("open rollout summaries dir");
    let mut files = Vec::new();
    while let Some(entry) = dir.next_entry().await.expect("read dir entry") {
        files.push(entry.file_name().to_string_lossy().to_string());
    }
    files.sort_unstable();
    assert_eq!(files.len(), 1);
    let canonical_rollout_summary_file = &files[0];

    let raw_memories = tokio::fs::read_to_string(raw_memories_file(&root))
        .await
        .expect("read raw memories");
    assert!(raw_memories.contains("raw memory"));
    assert!(raw_memories.contains(&keep_id));
    assert!(raw_memories.contains("cwd: /tmp/workspace"));
    assert!(raw_memories.contains("rollout_path: /tmp/rollout-100.jsonl"));
    assert!(raw_memories.contains(&format!(
        "rollout_summary_file: {canonical_rollout_summary_file}"
    )));
}

#[tokio::test]
async fn named_artifacts_use_stable_citation_and_complete_source_provenance() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("named-memory");
    let character_home = tempdir().expect("character home");
    write_character(character_home.path(), "chloe");
    let clanker_id = CanonicalClankerId::resolve_exact(
        &codex_character::CharacterCatalog::load(character_home.path()),
        "chloe",
    )
    .unwrap();
    let project_key = MemoryProjectKey::from_git_origin(
        "https://github.com/heiervang-technologies/clanker-code.git",
    )
    .unwrap();
    let thread_id = fixed_thread_id();
    let citation = MemoryCitationPath::new(format!("rollout_summaries/{thread_id}.md")).unwrap();
    let record = ScopedMemoryRecord {
        output: stage1_output_with_slug(thread_id, Some("ignored-for-citation")),
        clanker_id: Some(clanker_id.clone()),
        project_key: Some(project_key.clone()),
        visibility: MemoryVisibility::GlobalUserPreference,
        parent_thread_id: Some(ThreadId::default()),
        citation_path: Some(citation.clone()),
        usage_count: 0,
        last_usage: None,
    };
    let scope = MemorySelectionScope::Named {
        clanker_id,
        project_key,
    };

    sync_rollout_summaries_from_scoped_memories(&root, &scope, std::slice::from_ref(&record), 10)
        .await
        .unwrap();
    rebuild_raw_memories_file_from_scoped_memories(&root, &scope, &[record], 10)
        .await
        .unwrap();

    let summary = tokio::fs::read_to_string(root.join(citation.as_str()))
        .await
        .unwrap();
    for expected in [
        format!("thread_id: {thread_id}"),
        "source_character: chloe".to_string(),
        "source_project: v1:git:github.com/heiervang-technologies/clanker-code".to_string(),
        "visibility: global_user_preference".to_string(),
        format!("citation: {citation}"),
        "rollout_path: /tmp/rollout.jsonl".to_string(),
        "cwd: /tmp/workspace".to_string(),
    ] {
        assert!(summary.contains(&expected), "missing {expected:?}");
    }
    let raw = tokio::fs::read_to_string(raw_memories_file(&root))
        .await
        .unwrap();
    assert!(raw.contains(&format!("citation: {citation}")));
    assert!(raw.contains("raw memory"));
}

#[tokio::test]
async fn named_artifact_and_context_source_share_raw_memory_fallback() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("named-memory");
    let character_home = tempdir().expect("character home");
    write_character(character_home.path(), "chloe");
    let clanker_id = CanonicalClankerId::resolve_exact(
        &codex_character::CharacterCatalog::load(character_home.path()),
        "chloe",
    )
    .unwrap();
    let project_key = MemoryProjectKey::from_canonical_path("/workspace/project").unwrap();
    let thread_id = fixed_thread_id();
    let citation = MemoryCitationPath::new(format!("rollout_summaries/{thread_id}.md")).unwrap();
    let mut output = stage1_output_with_slug(thread_id, None);
    output.rollout_summary = " \n".to_string();
    output.raw_memory = "  raw fallback body  \n".to_string();
    let record = ScopedMemoryRecord {
        output,
        clanker_id: Some(clanker_id.clone()),
        project_key: Some(project_key.clone()),
        visibility: MemoryVisibility::PrivateCharacter,
        parent_thread_id: None,
        citation_path: Some(citation.clone()),
        usage_count: 0,
        last_usage: None,
    };

    sync_rollout_summaries_from_scoped_memories(
        &root,
        &MemorySelectionScope::Named {
            clanker_id,
            project_key,
        },
        std::slice::from_ref(&record),
        10,
    )
    .await
    .unwrap();

    let artifact = tokio::fs::read_to_string(root.join(citation.as_str()))
        .await
        .unwrap();
    assert_eq!(
        crate::scoped_episode_body(&record),
        Some("raw fallback body")
    );
    assert!(artifact.ends_with("\nraw fallback body\n"));
}

fn write_character(home: &std::path::Path, id: &str) {
    let avatar = home.join("characters").join(id).join("avatar");
    std::fs::create_dir_all(&avatar).unwrap();
    std::fs::write(
        avatar.parent().unwrap().join("character.json"),
        format!(r#"{{"schemaVersion":1,"id":"{id}","displayName":"{id}","avatar":"avatar/avatar.json"}}"#),
    )
    .unwrap();
    std::fs::write(
        avatar.join("avatar.json"),
        r#"{"renderMode":"ansi-half-block","spritesheetPath":"sheet.ppm","frame":{"width":24,"height":24,"columns":1,"rows":1}}"#,
    )
    .unwrap();
    std::fs::write(
        avatar.join("sheet.ppm"),
        format!("P3\n24 24\n255\n{}", "0 0 0\n".repeat(24 * 24)),
    )
    .unwrap();
}
