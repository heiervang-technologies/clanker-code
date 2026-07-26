use std::path::Path;
use std::sync::Arc;

use codex_extension_api::ContextContributor;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::NoopTurnItemEmitter;
use codex_extension_api::PromptSlot;
use codex_extension_api::ToolCall;
use codex_extension_api::ToolContributor;
use codex_extension_api::ToolExecutor;
use codex_extension_api::ToolName;
use codex_extension_api::ToolPayload;
use codex_tools::ToolOutput;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::PathExt;
use codex_utils_absolute_path::test_support::test_path_buf;
use codex_utils_output_truncation::TruncationPolicy;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::backend::MemoriesBackend;
use crate::extension::MemoriesExtension;
use crate::extension::MemoriesExtensionConfig;
use crate::local::LocalMemoriesBackend;
use crate::local::ScopedLocalMemoriesBackend;
use chrono::TimeZone;
use chrono::Utc;
use codex_character::CharacterCatalog;
use codex_state::CanonicalClankerId;
use codex_state::MemoryCitationPath;
use codex_state::MemoryProjectKey;
use codex_state::MemoryScope;
use codex_state::Stage1JobClaimOutcome;
use codex_state::Stage1MemoryPayload;
use codex_state::ThreadMetadataBuilder;

#[test]
fn memory_tool_namespace_matches_responses_api_identifier() {
    assert!(!crate::MEMORY_TOOLS_NAMESPACE.is_empty());
    assert!(
        crate::MEMORY_TOOLS_NAMESPACE
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    );
}

#[test]
fn tools_are_not_contributed_without_thread_config() {
    let extension = MemoriesExtension::default();

    assert!(
        extension
            .tools(
                &ExtensionData::new("session"),
                &ExtensionData::new("thread")
            )
            .is_empty()
    );
}

#[test]
fn tools_are_not_contributed_when_disabled() {
    let extension = MemoriesExtension::default();
    let thread_store = ExtensionData::new("thread");
    thread_store.insert(MemoriesExtensionConfig {
        enabled: false,
        dedicated_tools: true,
        codex_home: test_path_buf("/tmp/codex-home").abs(),
    });

    assert!(
        extension
            .tools(&ExtensionData::new("session"), &thread_store)
            .is_empty()
    );
}

#[test]
fn tools_are_not_contributed_when_dedicated_tools_disabled() {
    let extension = MemoriesExtension::default();
    let thread_store = ExtensionData::new("thread");
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: false,
        codex_home: test_path_buf("/tmp/codex-home").abs(),
    });

    assert!(
        extension
            .tools(&ExtensionData::new("session"), &thread_store)
            .is_empty()
    );
}

#[test]
fn tools_are_contributed_when_enabled_with_dedicated_tools() {
    let extension = MemoriesExtension::default();
    let thread_store = ExtensionData::new("thread");
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: true,
        codex_home: test_path_buf("/tmp/codex-home").abs(),
    });

    let tool_names = extension
        .tools(&ExtensionData::new("session"), &thread_store)
        .into_iter()
        .map(|tool| tool.tool_name())
        .collect::<Vec<_>>();

    assert_eq!(
        tool_names,
        vec![
            memory_tool_name(crate::ADD_AD_HOC_NOTE_TOOL_NAME),
            memory_tool_name(crate::LIST_TOOL_NAME),
            memory_tool_name(crate::READ_TOOL_NAME),
            memory_tool_name(crate::SEARCH_TOOL_NAME),
        ]
    );
}

#[test]
fn install_registers_dedicated_tool_contributor() {
    let mut builder = ExtensionRegistryBuilder::<codex_core::config::Config>::new();
    crate::install(
        &mut builder,
        /*metrics_client*/ None,
        /*state_db*/ None,
    );
    let registry = builder.build();
    let thread_store = ExtensionData::new("thread");
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: true,
        codex_home: test_path_buf("/tmp/codex-home").abs(),
    });

    let tool_names = registry
        .tool_contributors()
        .iter()
        .flat_map(|contributor| contributor.tools(&ExtensionData::new("session"), &thread_store))
        .map(|tool| tool.tool_name())
        .collect::<Vec<_>>();

    assert_eq!(
        tool_names,
        vec![
            memory_tool_name(crate::ADD_AD_HOC_NOTE_TOOL_NAME),
            memory_tool_name(crate::LIST_TOOL_NAME),
            memory_tool_name(crate::READ_TOOL_NAME),
            memory_tool_name(crate::SEARCH_TOOL_NAME),
        ]
    );
}

#[test]
fn ad_hoc_tool_definition_includes_filename_contract() {
    let tool = memory_tool(
        Path::new("/tmp/codex-home/memories"),
        crate::ADD_AD_HOC_NOTE_TOOL_NAME,
    );
    let spec = serde_json::to_value(tool.spec()).expect("serialize tool spec");

    let filename = spec
        .pointer("/tools/0/parameters/properties/filename")
        .expect("filename parameter should be in tool schema");
    assert_eq!(filename.pointer("/type"), Some(&json!("string")));
    assert!(
        filename
            .pointer("/description")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|description| description.contains("YYYY-MM-DDTHH-MM-SS-<slug>.md"))
    );
}

#[tokio::test]
async fn prompt_contribution_uses_memory_summary_when_enabled() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memories_dir = tempdir.path().join("memories");
    tokio::fs::create_dir_all(&memories_dir)
        .await
        .expect("create memories dir");
    tokio::fs::write(
        memories_dir.join("memory_summary.md"),
        "Remember repository-specific implementation preferences.",
    )
    .await
    .expect("write memory summary");

    let extension = MemoriesExtension::default();
    let thread_store = ExtensionData::new("thread");
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: false,
        codex_home: tempdir.path().abs(),
    });

    let fragments = extension
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;

    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].slot(), PromptSlot::DeveloperPolicy);
    assert!(
        fragments[0]
            .text()
            .contains("Remember repository-specific implementation preferences.")
    );
}

#[tokio::test]
async fn named_prompt_uses_db_selected_character_context_without_anonymous_summary() {
    let home = tempfile::tempdir().unwrap();
    write_character(home.path(), "chloe", "Chloe");
    let state_db = codex_state::StateRuntime::init(home.path().to_path_buf(), "test".to_string())
        .await
        .unwrap();
    let project = MemoryProjectKey::from_git_origin("git@github.com:example/project.git").unwrap();
    let current_thread = test_thread_id(20);
    let source_thread = test_thread_id(21);
    state_db
        .memories()
        .register_memory_scope(&MemoryScope {
            thread_id: current_thread,
            clanker_id: Some(canonical_id(home.path(), "chloe")),
            project_key: project.clone(),
            parent_thread_id: None,
            recorded_at: Utc.timestamp_opt(100, 0).unwrap(),
        })
        .await
        .unwrap();
    seed_scoped_output(
        state_db.as_ref(),
        home.path(),
        source_thread,
        "chloe",
        project.clone(),
        "named-context-only",
    )
    .await;
    let selection_scope = codex_state::MemorySelectionScope::Named {
        clanker_id: canonical_id(home.path(), "chloe"),
        project_key: project.clone(),
    };
    let selected = state_db
        .memories()
        .select_scoped_memories(&selection_scope, 128)
        .await
        .unwrap();
    let named_root =
        codex_memories_write::memory_root_for_scope(&home.path().abs(), &selection_scope);
    codex_memories_write::sync_rollout_summaries_from_scoped_memories(
        &named_root,
        &selection_scope,
        &selected,
        selected.len(),
    )
    .await
    .unwrap();
    tokio::fs::create_dir_all(home.path().join("memories"))
        .await
        .unwrap();
    tokio::fs::write(
        home.path().join("memories/memory_summary.md"),
        "anonymous-context-must-not-leak",
    )
    .await
    .unwrap();

    let extension = MemoriesExtension::new(None, Some(Arc::clone(&state_db)));
    let thread_store = ExtensionData::new(current_thread.to_string());
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: false,
        codex_home: home.path().abs(),
    });
    let fragments = extension
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;

    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].slot(), PromptSlot::ContextualUser);
    assert!(fragments[0].text().contains("<character_memory_context>"));
    assert!(fragments[0].text().contains("display_name=\"Chloe\""));
    assert!(fragments[0].text().contains("named-context-only"));
    assert!(
        !fragments[0]
            .text()
            .contains("anonymous-context-must-not-leak")
    );
    let selected_after = state_db
        .memories()
        .select_scoped_memories(&selection_scope, 128)
        .await
        .unwrap();
    assert_eq!(selected_after[0].usage_count, 0);
    assert!(selected_after[0].last_usage.is_none());

    let citation = selected_after[0].citation_path.as_ref().unwrap();
    tokio::fs::write(named_root.join(citation.as_str()), "stale artifact\n")
        .await
        .unwrap();
    let stale = extension
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;
    assert!(stale.is_empty(), "stale named artifact must not fall back");
    tokio::fs::remove_file(named_root.join(citation.as_str()))
        .await
        .unwrap();
    let missing = extension
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;
    assert!(
        missing.is_empty(),
        "missing named artifact must not fall back"
    );
}

#[tokio::test]
async fn unregistered_thread_with_state_preserves_legacy_anonymous_prompt_bytes() {
    let home = tempfile::tempdir().unwrap();
    tokio::fs::create_dir_all(home.path().join("memories"))
        .await
        .unwrap();
    tokio::fs::write(
        home.path().join("memories/memory_summary.md"),
        "legacy anonymous context",
    )
    .await
    .unwrap();
    let state_db = codex_state::StateRuntime::init(home.path().to_path_buf(), "test".to_string())
        .await
        .unwrap();
    let thread_store = ExtensionData::new(test_thread_id(22).to_string());
    thread_store.insert(MemoriesExtensionConfig {
        enabled: true,
        dedicated_tools: false,
        codex_home: home.path().abs(),
    });

    let legacy = MemoriesExtension::default()
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;
    let state_backed = MemoriesExtension::new(None, Some(state_db))
        .contribute_thread_context(&ExtensionData::new("session"), &thread_store)
        .await;

    assert_eq!(state_backed.len(), 1);
    assert_eq!(state_backed[0].slot(), PromptSlot::DeveloperPolicy);
    assert_eq!(state_backed[0].text(), legacy[0].text());
}

#[tokio::test]
async fn add_ad_hoc_note_tool_creates_note_file() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    let tool = memory_tool(&memory_root, crate::ADD_AD_HOC_NOTE_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "filename": "2026-05-26T13-42-08-remember-review-style.md",
            "note": "Remember to keep PR review comments concise.",
        })
        .to_string(),
    };

    let output = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::ADD_AD_HOC_NOTE_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload: payload.clone(),
        })
        .await
        .expect("ad-hoc note should be written");

    assert_eq!(
        output.post_tool_use_response("call-1", &payload),
        Some(json!({}))
    );
    assert_eq!(
        tokio::fs::read_to_string(
            memory_root
                .join("extensions/ad_hoc/notes")
                .join("2026-05-26T13-42-08-remember-review-style.md")
        )
        .await
        .expect("read ad-hoc note"),
        "Remember to keep PR review comments concise."
    );
}

#[tokio::test]
async fn add_ad_hoc_note_tool_rejects_paths_as_filenames() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    let tool = memory_tool(&memory_root, crate::ADD_AD_HOC_NOTE_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "filename": "../2026-05-26T13-42-08-remember-review-style.md",
            "note": "Remember to keep PR review comments concise.",
        })
        .to_string(),
    };

    let result = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::ADD_AD_HOC_NOTE_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload,
        })
        .await;
    let err = match result {
        Ok(_) => panic!("path-like filename should be rejected"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("filename"));
    assert!(err.to_string().contains("YYYY-MM-DDTHH-MM-SS"));
}

#[tokio::test]
async fn read_tool_reads_memory_file() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    tokio::fs::create_dir_all(&memory_root)
        .await
        .expect("create memories dir");
    tokio::fs::write(
        memory_root.join("MEMORY.md"),
        "first line\nsecond needle line\nthird line\n",
    )
    .await
    .expect("write memory");
    let tool = memory_tool(&memory_root, crate::READ_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "path": "MEMORY.md",
            "line_offset": 2,
            "max_lines": 1
        })
        .to_string(),
    };

    let output = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::READ_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload: payload.clone(),
        })
        .await
        .expect("read should succeed");

    assert_eq!(
        output.post_tool_use_response("call-1", &payload),
        Some(json!({
            "path": "MEMORY.md",
            "content": "second needle line\n",
            "start_line_number": 2,
            "truncated": true
        }))
    );
}

#[tokio::test]
async fn search_tool_accepts_multiple_queries() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    tokio::fs::create_dir_all(&memory_root)
        .await
        .expect("create memories dir");
    tokio::fs::write(
        memory_root.join("MEMORY.md"),
        "alpha only\nneedle only\nalpha needle\n",
    )
    .await
    .expect("write memory");
    let tool = memory_tool(&memory_root, crate::SEARCH_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "queries": ["alpha", "needle"],
            "case_sensitive": false
        })
        .to_string(),
    };

    let output = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::SEARCH_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload: payload.clone(),
        })
        .await
        .expect("search should succeed");

    assert_eq!(
        output.post_tool_use_response("call-1", &payload),
        Some(json!({
            "queries": ["alpha", "needle"],
            "match_mode": {
                "type": "any"
            },
            "path": null,
            "matches": [
                {
                    "path": "MEMORY.md",
                    "match_line_number": 1,
                    "content_start_line_number": 1,
                    "content": "alpha only",
                    "matched_queries": ["alpha"]
                },
                {
                    "path": "MEMORY.md",
                    "match_line_number": 2,
                    "content_start_line_number": 2,
                    "content": "needle only",
                    "matched_queries": ["needle"]
                },
                {
                    "path": "MEMORY.md",
                    "match_line_number": 3,
                    "content_start_line_number": 3,
                    "content": "alpha needle",
                    "matched_queries": ["alpha", "needle"]
                }
            ],
            "next_cursor": null,
            "truncated": false
        }))
    );
}

#[tokio::test]
async fn search_tool_accepts_windowed_all_match_mode() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    tokio::fs::create_dir_all(&memory_root)
        .await
        .expect("create memories dir");
    tokio::fs::write(memory_root.join("MEMORY.md"), "alpha\nmiddle\nneedle\n")
        .await
        .expect("write memory");
    let tool = memory_tool(&memory_root, crate::SEARCH_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "queries": ["alpha", "needle"],
            "match_mode": {
                "type": "all_within_lines",
                "line_count": 3
            }
        })
        .to_string(),
    };

    let output = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::SEARCH_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload: payload.clone(),
        })
        .await
        .expect("search should succeed");

    assert_eq!(
        output.post_tool_use_response("call-1", &payload),
        Some(json!({
            "queries": ["alpha", "needle"],
            "match_mode": {
                "type": "all_within_lines",
                "line_count": 3
            },
            "path": null,
            "matches": [
                {
                    "path": "MEMORY.md",
                    "match_line_number": 1,
                    "content_start_line_number": 1,
                    "content": "alpha\nmiddle\nneedle",
                    "matched_queries": ["alpha", "needle"]
                }
            ],
            "next_cursor": null,
            "truncated": false
        }))
    );
}

#[tokio::test]
async fn search_tool_rejects_legacy_single_query() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let memory_root = tempdir.path().join("memories");
    tokio::fs::create_dir_all(&memory_root)
        .await
        .expect("create memories dir");
    let tool = memory_tool(&memory_root, crate::SEARCH_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({
            "query": "needle",
        })
        .to_string(),
    };

    let result = tool
        .handle(ToolCall {
            turn_id: "turn-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: memory_tool_name(crate::SEARCH_TOOL_NAME),
            model: "gpt-test".to_string(),
            codex_turn_metadata: None,
            truncation_policy: TruncationPolicy::Bytes(1024),
            conversation_history: codex_extension_api::ConversationHistory::default(),
            turn_item_emitter: Arc::new(NoopTurnItemEmitter),
            environments: Vec::new(),
            payload,
        })
        .await;
    let err = match result {
        Ok(_) => panic!("legacy query field should be rejected"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("unknown field"));
    assert!(err.to_string().contains("query"));
}

#[tokio::test]
async fn scoped_tools_isolate_named_characters_and_anonymous_legacy() {
    let home = tempfile::tempdir().unwrap();
    write_character(home.path(), "chloe", "Chloe");
    write_character(home.path(), "clanker", "Clanker");
    let state_db = codex_state::StateRuntime::init(home.path().to_path_buf(), "test".to_string())
        .await
        .unwrap();
    let project = MemoryProjectKey::from_git_origin("git@github.com:example/project.git").unwrap();
    let chloe_thread = test_thread_id(1);
    let clanker_thread = test_thread_id(2);
    for (thread_id, character) in [(chloe_thread, "chloe"), (clanker_thread, "clanker")] {
        state_db
            .memories()
            .register_memory_scope(&MemoryScope {
                thread_id,
                clanker_id: Some(canonical_id(home.path(), character)),
                project_key: project.clone(),
                parent_thread_id: None,
                recorded_at: Utc.timestamp_opt(100, 0).unwrap(),
            })
            .await
            .unwrap();
    }
    let chloe_root = named_memory_root(home.path(), "chloe", &project);
    let clanker_root = named_memory_root(home.path(), "clanker", &project);
    let anonymous_root = home.path().join("memories");
    for (root, text) in [
        (&chloe_root, "chloe-only"),
        (&clanker_root, "clanker-only"),
        (&anonymous_root, "anonymous-only"),
    ] {
        tokio::fs::create_dir_all(root).await.unwrap();
        tokio::fs::write(root.join("MEMORY.md"), text)
            .await
            .unwrap();
        tokio::fs::write(root.join(format!("{text}.md")), text)
            .await
            .unwrap();
    }

    let chloe = ScopedLocalMemoriesBackend::new(
        home.path().abs(),
        Some(Arc::clone(&state_db)),
        chloe_thread,
    );
    let clanker = ScopedLocalMemoriesBackend::new(
        home.path().abs(),
        Some(Arc::clone(&state_db)),
        clanker_thread,
    );
    let anonymous = ScopedLocalMemoriesBackend::new(
        home.path().abs(),
        Some(Arc::clone(&state_db)),
        test_thread_id(3),
    );

    assert_eq!(read_tool_text(chloe.clone()).await, "chloe-only");
    assert_eq!(read_tool_text(clanker.clone()).await, "clanker-only");
    assert_eq!(read_tool_text(anonymous.clone()).await, "anonymous-only");
    assert_eq!(
        search_tool_match_count(chloe.clone(), "chloe-only").await,
        2
    );
    assert_eq!(
        search_tool_match_count(chloe.clone(), "clanker-only").await,
        0
    );
    assert_eq!(
        search_tool_match_count(clanker.clone(), "clanker-only").await,
        2
    );
    assert_eq!(
        search_tool_match_count(clanker.clone(), "anonymous-only").await,
        0
    );
    assert_eq!(
        search_tool_match_count(anonymous.clone(), "anonymous-only").await,
        2
    );
    assert_eq!(
        search_tool_match_count(anonymous.clone(), "chloe-only").await,
        0
    );
    for (backend, own, foreign) in [
        (chloe.clone(), "chloe-only.md", "clanker-only.md"),
        (clanker.clone(), "clanker-only.md", "anonymous-only.md"),
        (anonymous.clone(), "anonymous-only.md", "chloe-only.md"),
    ] {
        let paths = list_tool_entry_paths(backend).await;
        assert!(paths.iter().any(|path| path == own));
        assert!(!paths.iter().any(|path| path == foreign));
    }

    for (backend, root, filename) in [
        (chloe, &chloe_root, "2026-07-23T02-00-00-chloe.md"),
        (clanker, &clanker_root, "2026-07-23T02-00-01-clanker.md"),
        (
            anonymous,
            &anonymous_root,
            "2026-07-23T02-00-02-anonymous.md",
        ),
    ] {
        run_ad_hoc_tool(backend, filename).await;
        assert!(
            root.join("extensions/ad_hoc/notes")
                .join(filename)
                .is_file()
        );
    }
    assert!(
        !chloe_root
            .join("extensions/ad_hoc/notes/2026-07-23T02-00-01-clanker.md")
            .exists()
    );
    assert!(
        !clanker_root
            .join("extensions/ad_hoc/notes/2026-07-23T02-00-02-anonymous.md")
            .exists()
    );
    assert!(
        !anonymous_root
            .join("extensions/ad_hoc/notes/2026-07-23T02-00-00-chloe.md")
            .exists()
    );
}

#[tokio::test]
async fn named_tool_scope_failure_never_falls_back_to_anonymous() {
    let home = tempfile::tempdir().unwrap();
    write_character(home.path(), "chloe", "Chloe");
    let state_db = codex_state::StateRuntime::init(home.path().to_path_buf(), "test".to_string())
        .await
        .unwrap();
    let thread_id = test_thread_id(10);
    state_db
        .memories()
        .register_memory_scope(&MemoryScope {
            thread_id,
            clanker_id: Some(canonical_id(home.path(), "chloe")),
            project_key: MemoryProjectKey::from_canonical_path(
                home.path()
                    .canonicalize()
                    .unwrap()
                    .join("workspace")
                    .join("project"),
            )
            .unwrap(),
            parent_thread_id: None,
            recorded_at: Utc.timestamp_opt(100, 0).unwrap(),
        })
        .await
        .unwrap();
    tokio::fs::create_dir_all(home.path().join("memories"))
        .await
        .unwrap();
    tokio::fs::write(home.path().join("memories/MEMORY.md"), "must-not-leak")
        .await
        .unwrap();
    let backend =
        ScopedLocalMemoriesBackend::new(home.path().abs(), Some(Arc::clone(&state_db)), thread_id);
    state_db.close().await;

    let tool = memory_tool_with_backend(backend, crate::READ_TOOL_NAME);
    let payload = ToolPayload::Function {
        arguments: json!({"path": "MEMORY.md"}).to_string(),
    };
    let error = match tool.handle(tool_call(crate::READ_TOOL_NAME, payload)).await {
        Ok(_) => panic!("closed named scope must fail without anonymous fallback"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("immutable memory scope"));
    assert!(!error.to_string().contains("must-not-leak"));
}

fn memory_tool(memory_root: &Path, tool_name: &str) -> Arc<dyn ToolExecutor<ToolCall>> {
    memory_tool_with_backend(
        LocalMemoriesBackend::from_memory_root(memory_root),
        tool_name,
    )
}

fn memory_tool_with_backend<B>(backend: B, tool_name: &str) -> Arc<dyn ToolExecutor<ToolCall>>
where
    B: MemoriesBackend,
{
    let expected_tool_name = memory_tool_name(tool_name);
    crate::tools::memory_tools(backend, /*metrics_client*/ None)
        .into_iter()
        .find(|tool| tool.tool_name() == expected_tool_name)
        .unwrap_or_else(|| panic!("{tool_name} tool should be registered"))
}

fn tool_call(tool_name: &str, payload: ToolPayload) -> ToolCall {
    ToolCall {
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        tool_name: memory_tool_name(tool_name),
        model: "gpt-test".to_string(),
        codex_turn_metadata: None,
        truncation_policy: TruncationPolicy::Bytes(4096),
        conversation_history: codex_extension_api::ConversationHistory::default(),
        turn_item_emitter: Arc::new(NoopTurnItemEmitter),
        environments: Vec::new(),
        payload,
    }
}

async fn read_tool_text<B: MemoriesBackend>(backend: B) -> String {
    let payload = ToolPayload::Function {
        arguments: json!({"path": "MEMORY.md"}).to_string(),
    };
    let output = memory_tool_with_backend(backend, crate::READ_TOOL_NAME)
        .handle(tool_call(crate::READ_TOOL_NAME, payload.clone()))
        .await
        .unwrap();
    output.post_tool_use_response("call-1", &payload).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn search_tool_match_count<B: MemoriesBackend>(backend: B, query: &str) -> usize {
    let payload = ToolPayload::Function {
        arguments: json!({"queries": [query]}).to_string(),
    };
    let output = memory_tool_with_backend(backend, crate::SEARCH_TOOL_NAME)
        .handle(tool_call(crate::SEARCH_TOOL_NAME, payload.clone()))
        .await
        .unwrap();
    output.post_tool_use_response("call-1", &payload).unwrap()["matches"]
        .as_array()
        .unwrap()
        .len()
}

async fn list_tool_entry_paths<B: MemoriesBackend>(backend: B) -> Vec<String> {
    let payload = ToolPayload::Function {
        arguments: "{}".to_string(),
    };
    let output = memory_tool_with_backend(backend, crate::LIST_TOOL_NAME)
        .handle(tool_call(crate::LIST_TOOL_NAME, payload.clone()))
        .await
        .unwrap();
    output.post_tool_use_response("call-1", &payload).unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

async fn run_ad_hoc_tool<B: MemoriesBackend>(backend: B, filename: &str) {
    let payload = ToolPayload::Function {
        arguments: json!({"filename": filename, "note": "scoped note"}).to_string(),
    };
    memory_tool_with_backend(backend, crate::ADD_AD_HOC_NOTE_TOOL_NAME)
        .handle(tool_call(crate::ADD_AD_HOC_NOTE_TOOL_NAME, payload))
        .await
        .unwrap();
}

fn named_memory_root(home: &Path, id: &str, project: &MemoryProjectKey) -> std::path::PathBuf {
    codex_memories_write::memory_root_for_scope(
        &home.abs(),
        &codex_state::MemorySelectionScope::Named {
            clanker_id: canonical_id(home, id),
            project_key: project.clone(),
        },
    )
    .to_path_buf()
}

fn canonical_id(home: &Path, id: &str) -> CanonicalClankerId {
    CanonicalClankerId::resolve_exact(&CharacterCatalog::load(home), id).unwrap()
}

fn write_character(home: &Path, id: &str, display_name: &str) {
    let package = home.join("characters").join(id);
    let avatar = package.join("avatar");
    std::fs::create_dir_all(&avatar).unwrap();
    std::fs::write(
        package.join("character.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "id": id,
            "displayName": display_name,
            "avatar": "avatar/avatar.json"
        }))
        .unwrap(),
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

async fn seed_scoped_output(
    state_db: &codex_state::StateRuntime,
    home: &Path,
    thread_id: codex_protocol::ThreadId,
    character: &str,
    project_key: MemoryProjectKey,
    summary: &str,
) {
    let mut metadata = ThreadMetadataBuilder::new(
        thread_id,
        home.join("rollouts").join(format!("{thread_id}.jsonl")),
        Utc.timestamp_opt(100, 0).unwrap(),
        codex_protocol::protocol::SessionSource::Cli,
    );
    metadata.cwd = home.join("workspace");
    state_db
        .upsert_thread(&metadata.build("test"))
        .await
        .unwrap();
    let claim = state_db
        .memories()
        .try_claim_stage1_job(thread_id, test_thread_id(999), 100, 3_600, 64)
        .await
        .unwrap();
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        panic!("unexpected stage-1 claim: {claim:?}");
    };
    let scope = MemoryScope {
        thread_id,
        clanker_id: Some(canonical_id(home, character)),
        project_key,
        parent_thread_id: None,
        recorded_at: Utc.timestamp_opt(100, 0).unwrap(),
    };
    assert!(
        state_db
            .memories()
            .mark_stage1_job_succeeded_scoped(
                &scope,
                &MemoryCitationPath::new(format!("rollout_summaries/{thread_id}.md")).unwrap(),
                &ownership_token,
                Stage1MemoryPayload {
                    source_updated_at: 100,
                    raw_memory: "raw",
                    rollout_summary: summary,
                    rollout_slug: None,
                },
            )
            .await
            .unwrap()
    );
}

fn test_thread_id(value: u128) -> codex_protocol::ThreadId {
    codex_protocol::ThreadId::from_string(uuid::Uuid::from_u128(value).to_string().as_str())
        .unwrap()
}

fn memory_tool_name(tool_name: &str) -> ToolName {
    ToolName::namespaced(crate::MEMORY_TOOLS_NAMESPACE, tool_name)
}
