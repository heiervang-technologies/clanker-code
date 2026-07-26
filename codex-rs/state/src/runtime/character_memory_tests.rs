use super::StateRuntime;
use crate::CanonicalClankerId;
use crate::MemoryCitationPath;
use crate::MemoryProjectKey;
use crate::MemoryScope;
use crate::MemoryScopeError;
use crate::MemoryScopeRegistration;
use crate::MemorySelectionScope;
use crate::MemoryVisibility;
use crate::Phase2JobClaimOutcome;
use crate::Stage1JobClaimOutcome;
use crate::ThreadMetadataBuilder;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use codex_character::CharacterCatalog;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn project_keys_strip_credentials_and_normalize_equivalent_origins() {
    let https = MemoryProjectKey::from_git_origin(
        "https://user:secret@GitHub.COM/Heiervang-Technologies/Clanker-Code.git?token=nope#frag",
    )
    .expect("HTTPS origin should canonicalize");
    let ssh = MemoryProjectKey::from_git_origin(
        "ssh://git@github.com:22/heiervang-technologies/clanker-code.git",
    )
    .expect("SSH origin should canonicalize");
    let scp = MemoryProjectKey::from_git_origin(
        "token@github.com:heiervang-technologies/clanker-code.git?secret=no#fragment",
    )
    .expect("scp origin should canonicalize");
    let bare = MemoryProjectKey::from_git_origin(
        "token@github.com/heiervang-technologies/clanker-code.git?secret=no#fragment",
    )
    .expect("bare origin should canonicalize");

    assert_eq!(https, ssh);
    assert_eq!(https, scp);
    assert_eq!(https, bare);
    assert_eq!(
        https.as_str(),
        "v1:git:github.com/heiervang-technologies/clanker-code"
    );
    assert!(!https.as_str().contains("secret"));
    assert!(!https.as_str().contains("token"));
    assert!(!https.as_str().contains("secret"));
    assert_eq!(https.artifact_digest().len(), 64);
}

#[test]
fn path_project_keys_are_explicitly_path_bound() {
    let project_root = TempDir::new().expect("project root");
    let canonical_root = project_root.path().canonicalize().expect("canonical root");
    let first_path = canonical_root.join("first");
    let renamed_path = canonical_root.join("renamed");
    let first = MemoryProjectKey::from_canonical_path(&first_path)
        .expect("absolute path should be accepted");
    let renamed = MemoryProjectKey::from_canonical_path(&renamed_path)
        .expect("absolute path should be accepted");

    assert_ne!(first, renamed);
    assert_eq!(first.as_str(), format!("v1:path:{}", first_path.display()));
    assert!(MemoryProjectKey::from_canonical_path("relative").is_err());
    assert!(MemoryProjectKey::from_canonical_path(project_root.path().join("a/../b")).is_err());
    assert!(MemoryProjectKey::from_canonical_path(project_root.path().join("a/./b")).is_err());
    let duplicate_separator = format!(
        "{}{separator}{separator}a",
        canonical_root.display(),
        separator = std::path::MAIN_SEPARATOR
    );
    assert!(MemoryProjectKey::from_canonical_path(duplicate_separator).is_err());
    assert!(MemoryProjectKey::from_canonical_path(canonical_root.join("a\nb")).is_err());
}

#[test]
fn canonical_identity_requires_an_exact_catalog_match() {
    let (_home, catalog) = catalog_with_character("chloe", &["cleo"]);

    assert_eq!(
        CanonicalClankerId::resolve_exact(&catalog, "chloe")
            .expect("exact canonical id")
            .as_str(),
        "chloe"
    );
    for rejected in ["Chloe", "CHLOE", "cleo", "unknown"] {
        assert!(matches!(
            CanonicalClankerId::resolve_exact(&catalog, rejected),
            Err(MemoryScopeError::UnverifiedCanonicalClankerId { id }) if id == rejected
        ));
    }
}

#[test]
fn citation_paths_reject_traversal_roots_dot_segments_and_controls() {
    assert_eq!(
        MemoryCitationPath::new("rollout_summaries/thread.md")
            .unwrap()
            .as_str(),
        "rollout_summaries/thread.md"
    );
    for rejected in [
        "",
        "/absolute.md",
        "../escape.md",
        "rollout_summaries/./thread.md",
        "rollout_summaries/../thread.md",
        "rollout_summaries/thread\n.md",
    ] {
        assert!(MemoryCitationPath::new(rejected).is_err(), "{rejected:?}");
    }
}

#[tokio::test]
async fn scope_registration_is_immutable_and_exact_replay_is_idempotent() {
    let (_home, runtime) = runtime().await;
    let source_thread = thread_id(1);
    let scope = named_scope(
        source_thread,
        "chloe",
        project_a(),
        /*parent*/ None,
        100,
    );

    assert_eq!(
        runtime
            .memories()
            .register_memory_scope(&scope)
            .await
            .unwrap(),
        MemoryScopeRegistration::Inserted
    );
    let replay_with_later_timestamp = MemoryScope {
        recorded_at: timestamp(200),
        ..scope.clone()
    };
    assert_eq!(
        runtime
            .memories()
            .register_memory_scope(&replay_with_later_timestamp)
            .await
            .unwrap(),
        MemoryScopeRegistration::ReplayedExact
    );

    let conflict = named_scope(
        source_thread,
        "clanker",
        project_a(),
        /*parent*/ None,
        100,
    );
    assert!(matches!(
        runtime.memories().register_memory_scope(&conflict).await,
        Err(MemoryScopeError::ScopeConflict { thread_id: id }) if id == source_thread
    ));

    let project_conflict = named_scope(
        source_thread,
        "chloe",
        project_b(),
        /*parent*/ None,
        100,
    );
    assert!(matches!(
        runtime
            .memories()
            .register_memory_scope(&project_conflict)
            .await,
        Err(MemoryScopeError::ScopeConflict { thread_id: id }) if id == source_thread
    ));
    let parent_conflict = named_scope(source_thread, "chloe", project_a(), Some(thread_id(2)), 100);
    assert!(matches!(
        runtime
            .memories()
            .register_memory_scope(&parent_conflict)
            .await,
        Err(MemoryScopeError::ScopeConflict { thread_id: id }) if id == source_thread
    ));

    let anonymous_thread = thread_id(3);
    runtime
        .memories()
        .register_memory_scope(&anonymous_scope(anonymous_thread, project_a(), 100))
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .memories()
            .register_memory_scope(&named_scope(
                anonymous_thread,
                "chloe",
                project_a(),
                None,
                100,
            ))
            .await,
        Err(MemoryScopeError::ScopeConflict { thread_id: id }) if id == anonymous_thread
    ));

    let stored = runtime
        .memories()
        .memory_scope(source_thread)
        .await
        .unwrap();
    assert_eq!(stored, Some(scope));
}

#[tokio::test]
async fn selector_isolates_characters_and_projects_with_stable_visibility_tiers() {
    let (_home, runtime) = runtime().await;
    let chloe_a = thread_id(10);
    let clanker_a_shared = thread_id(11);
    let clanker_b_global = thread_id(12);
    let chloe_b = thread_id(13);
    let anonymous = thread_id(14);

    seed_output(
        &runtime,
        named_scope(chloe_a, "chloe", project_a(), None, 100),
        "chloe private a",
    )
    .await;
    seed_output(
        &runtime,
        named_scope(clanker_a_shared, "clanker", project_a(), None, 100),
        "clanker shared a",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(clanker_a_shared, MemoryVisibility::ProjectShared)
        .await
        .unwrap();
    seed_output(
        &runtime,
        named_scope(clanker_b_global, "clanker", project_b(), None, 100),
        "clanker global b",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(clanker_b_global, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();
    seed_output(
        &runtime,
        named_scope(chloe_b, "chloe", project_b(), None, 100),
        "chloe private b",
    )
    .await;
    seed_output(
        &runtime,
        anonymous_scope(anonymous, project_a(), 100),
        "anonymous legacy",
    )
    .await;

    let selected = runtime
        .memories()
        .select_scoped_memories(
            &MemorySelectionScope::Named {
                clanker_id: canonical_id("chloe"),
                project_key: project_a(),
            },
            10,
        )
        .await
        .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|record| (
                record.output.thread_id,
                record.visibility,
                record.clanker_id.as_ref().map(CanonicalClankerId::as_str),
            ))
            .collect::<Vec<_>>(),
        vec![
            (chloe_a, MemoryVisibility::PrivateCharacter, Some("chloe")),
            (
                clanker_a_shared,
                MemoryVisibility::ProjectShared,
                Some("clanker")
            ),
            (
                clanker_b_global,
                MemoryVisibility::GlobalUserPreference,
                Some("clanker")
            ),
        ]
    );
    assert_eq!(
        selected
            .iter()
            .map(|record| record.citation_path.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(citation_path(chloe_a)),
            Some(citation_path(clanker_a_shared)),
            Some(citation_path(clanker_b_global)),
        ]
    );

    let anonymous_selected = runtime
        .memories()
        .select_scoped_memories(&MemorySelectionScope::Anonymous, 10)
        .await
        .unwrap();
    assert_eq!(
        anonymous_selected
            .iter()
            .map(|record| record.output.thread_id)
            .collect::<Vec<_>>(),
        vec![anonymous]
    );
}

#[tokio::test]
async fn global_selection_prefers_current_project_before_usage_and_recency() {
    let (_home, runtime) = runtime().await;
    let current_project = thread_id(15);
    let foreign_strong = thread_id(16);
    let foreign_weak = thread_id(17);
    for (scope, body) in [
        (
            named_scope(current_project, "clanker", project_a(), None, 100),
            "current project",
        ),
        (
            named_scope(foreign_strong, "orion", project_b(), None, 300),
            "foreign strong",
        ),
        (
            named_scope(foreign_weak, "warren", project_b(), None, 200),
            "foreign weak",
        ),
    ] {
        seed_output(&runtime, scope, body).await;
    }
    for source in [current_project, foreign_strong, foreign_weak] {
        runtime
            .memories()
            .set_trusted_memory_visibility(source, MemoryVisibility::GlobalUserPreference)
            .await
            .unwrap();
    }
    runtime
        .memories()
        .record_stage1_output_usage(&[foreign_strong])
        .await
        .unwrap();

    let selected = runtime
        .memories()
        .select_scoped_memories(&named_selection("chloe", project_a()), 10)
        .await
        .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|record| record.output.thread_id)
            .collect::<Vec<_>>(),
        vec![current_project, foreign_strong, foreign_weak]
    );
}

#[tokio::test]
async fn named_binding_and_selection_require_typed_citations() {
    let (_anonymous_home, anonymous_runtime) = runtime().await;
    let (_home, runtime) = runtime().await;
    let missing_at_bind = thread_id(18);
    seed_unbound_output(
        &runtime,
        named_scope(missing_at_bind, "chloe", project_a(), None, 100),
        "missing citation",
    )
    .await;
    assert!(matches!(
        runtime
            .memories()
            .bind_registered_scope_to_stage1_output(missing_at_bind, None)
            .await,
        Err(MemoryScopeError::MissingCitation { thread_id }) if thread_id == missing_at_bind
    ));

    let corrupted = thread_id(19);
    seed_output(
        &runtime,
        named_scope(corrupted, "chloe", project_a(), None, 100),
        "corrupted citation",
    )
    .await;
    sqlx::query("UPDATE stage1_outputs SET citation_path = NULL WHERE thread_id = ?")
        .bind(corrupted.to_string())
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .memories()
            .select_scoped_memories(&named_selection("chloe", project_a()), 10)
            .await,
        Err(MemoryScopeError::MissingCitation { thread_id }) if thread_id == corrupted
    ));

    let anonymous = thread_id(24);
    seed_unbound_output(
        &anonymous_runtime,
        anonymous_scope(anonymous, project_a(), 100),
        "anonymous",
    )
    .await;
    assert!(
        anonymous_runtime
            .memories()
            .bind_registered_scope_to_stage1_output(anonymous, None)
            .await
            .unwrap()
    );
    let selected = anonymous_runtime
        .memories()
        .select_scoped_memories(&MemorySelectionScope::Anonymous, 10)
        .await
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].output.thread_id, anonymous);
    assert_eq!(selected[0].citation_path, None);
}

#[tokio::test]
async fn scoped_resets_preserve_bindings_until_all_local() {
    let (_home, runtime) = runtime().await;
    let chloe_a = thread_id(20);
    let chloe_b = thread_id(21);
    let clanker_a = thread_id(22);
    let anonymous = thread_id(23);
    for (scope, body) in [
        (
            named_scope(chloe_a, "chloe", project_a(), None, 100),
            "chloe a",
        ),
        (
            named_scope(chloe_b, "chloe", project_b(), None, 100),
            "chloe b",
        ),
        (
            named_scope(clanker_a, "clanker", project_a(), None, 100),
            "clanker a",
        ),
        (anonymous_scope(anonymous, project_a(), 100), "anonymous"),
    ] {
        seed_output(&runtime, scope, body).await;
    }
    runtime
        .memories()
        .set_trusted_memory_visibility(chloe_b, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();

    let thread_receipt = runtime
        .memories()
        .reset_thread_memory(chloe_a)
        .await
        .unwrap();
    assert_eq!(thread_receipt.removed_outputs, 1);
    assert_eq!(
        thread_receipt.affected_scopes,
        vec![named_selection("chloe", project_a())]
    );
    assert_eq!(
        runtime.memories().memory_scope(chloe_a).await.unwrap(),
        Some(named_scope(chloe_a, "chloe", project_a(), None, 100))
    );

    let character_receipt = runtime
        .memories()
        .reset_character_memory_from_thread(chloe_b)
        .await
        .unwrap();
    assert_eq!(character_receipt.removed_outputs, 1);
    assert_eq!(
        character_receipt.affected_scopes,
        vec![
            named_selection("chloe", project_a()),
            named_selection("chloe", project_b()),
            named_selection("clanker", project_a()),
        ]
    );
    assert!(
        runtime
            .memories()
            .memory_scope(chloe_b)
            .await
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        runtime
            .memories()
            .reset_character_memory_from_thread(anonymous)
            .await,
        Err(MemoryScopeError::AnonymousScope { thread_id: id }) if id == anonymous
    ));

    let clanker = runtime
        .memories()
        .select_scoped_memories(
            &MemorySelectionScope::Named {
                clanker_id: canonical_id("clanker"),
                project_key: project_a(),
            },
            10,
        )
        .await
        .unwrap();
    assert_eq!(
        clanker
            .iter()
            .map(|record| record.output.thread_id)
            .collect::<Vec<_>>(),
        vec![clanker_a]
    );

    runtime.memories().clear_memory_data().await.unwrap();
    assert_eq!(
        runtime.memories().memory_scope(clanker_a).await.unwrap(),
        None
    );
    assert!(
        runtime
            .memories()
            .select_scoped_memories(&MemorySelectionScope::Anonymous, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn shared_and_global_reset_receipts_cover_every_registered_consumer_scope() {
    let (_home, runtime) = runtime().await;
    let shared_source = thread_id(40);
    let global_source = thread_id(41);
    let chloe_a = thread_id(42);
    let orion_a = thread_id(43);
    let chloe_b = thread_id(44);

    seed_output(
        &runtime,
        named_scope(shared_source, "clanker", project_a(), None, 100),
        "shared",
    )
    .await;
    for scope in [
        named_scope(chloe_a, "chloe", project_a(), None, 100),
        named_scope(orion_a, "orion", project_a(), None, 100),
        named_scope(chloe_b, "chloe", project_b(), None, 100),
    ] {
        register_scope_only(&runtime, scope).await;
    }
    runtime
        .memories()
        .set_trusted_memory_visibility(shared_source, MemoryVisibility::ProjectShared)
        .await
        .unwrap();

    let shared_receipt = runtime
        .memories()
        .reset_thread_memory(shared_source)
        .await
        .unwrap();
    assert_eq!(
        shared_receipt.affected_scopes,
        vec![
            named_selection("chloe", project_a()),
            named_selection("clanker", project_a()),
            named_selection("orion", project_a()),
        ]
    );

    seed_output(
        &runtime,
        named_scope(global_source, "clanker", project_b(), None, 100),
        "global",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(global_source, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();
    let global_receipt = runtime
        .memories()
        .reset_thread_memory(global_source)
        .await
        .unwrap();
    assert_eq!(
        global_receipt.affected_scopes,
        vec![
            named_selection("chloe", project_a()),
            named_selection("chloe", project_b()),
            named_selection("clanker", project_a()),
            named_selection("clanker", project_b()),
            named_selection("orion", project_a()),
        ]
    );
}

#[tokio::test]
async fn shared_and_global_rekey_receipt_covers_old_and_new_consumer_scopes() {
    let (_home, runtime) = runtime().await;
    let shared_source = thread_id(50);
    let global_source = thread_id(51);
    seed_output(
        &runtime,
        named_scope(shared_source, "lawrence", project_a(), None, 100),
        "shared",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(shared_source, MemoryVisibility::ProjectShared)
        .await
        .unwrap();
    seed_output(
        &runtime,
        named_scope(global_source, "lawrence", project_b(), None, 100),
        "global",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(global_source, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();
    register_scope_only(
        &runtime,
        named_scope(thread_id(52), "chloe", project_a(), None, 100),
    )
    .await;
    register_scope_only(
        &runtime,
        named_scope(thread_id(53), "orion", project_b(), None, 100),
    )
    .await;

    let receipt = runtime
        .memories()
        .rekey_character_memory(&canonical_id("lawrence"), &canonical_id("warren"))
        .await
        .unwrap();
    assert_eq!(receipt.updated_scopes, 2);
    assert_eq!(receipt.updated_outputs, 2);
    assert_eq!(
        receipt.affected_scopes,
        vec![
            named_selection("chloe", project_a()),
            named_selection("lawrence", project_a()),
            named_selection("lawrence", project_b()),
            named_selection("orion", project_b()),
            named_selection("warren", project_a()),
            named_selection("warren", project_b()),
        ]
    );
}

#[tokio::test]
async fn scoped_selection_repeats_byte_stable_tie_order() {
    let (_home, runtime) = runtime().await;
    for source in [thread_id(60), thread_id(61), thread_id(62)] {
        seed_output(
            &runtime,
            named_scope(source, "chloe", project_a(), None, 100),
            "tie",
        )
        .await;
    }
    let scope = named_selection("chloe", project_a());
    let expected = [thread_id(60), thread_id(61), thread_id(62)]
        .map(|thread_id| thread_id.to_string())
        .join("\n")
        .into_bytes();
    for _ in 0..5 {
        let selected = runtime
            .memories()
            .select_scoped_memories(&scope, 10)
            .await
            .unwrap();
        let bytes = selected
            .iter()
            .map(|record| record.output.thread_id.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        assert_eq!(bytes, expected);
    }
}

#[tokio::test]
async fn selection_scan_is_bounded_and_corrupt_provenance_never_enters_context() {
    let (_home, runtime) = runtime().await;
    let valid = thread_id(70);
    seed_output(
        &runtime,
        named_scope(valid, "chloe", project_a(), None, 1),
        "valid after scan cap",
    )
    .await;

    let mut tx = runtime.memories().pool_for_tests().begin().await.unwrap();
    for offset in 0..128_u128 {
        sqlx::query(
            r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, raw_memory, rollout_summary, generated_at,
    clanker_id, project_key, visibility, citation_path
) VALUES (?, ?, 'disabled', 'disabled', ?, 'chloe', ?, 'private_character', ?)
            "#,
        )
        .bind(thread_id(10_000 + offset).to_string())
        .bind(1_000 + i64::try_from(offset).unwrap())
        .bind(1_000 + i64::try_from(offset).unwrap())
        .bind(project_a().as_str())
        .bind("rollout_summaries/disabled.md")
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    for (source, visibility) in [
        (thread_id(71), "project_shared"),
        (thread_id(72), "global_user_preference"),
    ] {
        seed_thread(&runtime, source, timestamp(2_000)).await;
        sqlx::query(
            r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, raw_memory, rollout_summary, generated_at,
    clanker_id, project_key, visibility, citation_path
) VALUES (?, 2000, 'corrupt', 'corrupt', 2000, NULL, NULL, ?, ?)
            "#,
        )
        .bind(source.to_string())
        .bind(visibility)
        .bind("rollout_summaries/corrupt.md")
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let selected = runtime
        .memories()
        .select_scoped_memories(&named_selection("chloe", project_a()), 10)
        .await
        .unwrap();
    assert!(selected.is_empty(), "bounded scan intentionally underfills");

    sqlx::query("UPDATE stage1_outputs SET citation_path = '../escape.md' WHERE thread_id = ?")
        .bind(valid.to_string())
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();
    sqlx::query("DELETE FROM stage1_outputs WHERE raw_memory = 'disabled'")
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .memories()
            .select_scoped_memories(&named_selection("chloe", project_a()), 10)
            .await,
        Err(MemoryScopeError::InvalidCitationPath)
    ));

    let invalid_visibility = sqlx::query(
        r#"
INSERT INTO stage1_outputs (
    thread_id, source_updated_at, raw_memory, rollout_summary, generated_at, visibility
) VALUES (?, 1, 'bad', 'bad', 1, 'untrusted')
        "#,
    )
    .bind(thread_id(73).to_string())
    .execute(runtime.memories().pool_for_tests())
    .await;
    assert!(
        invalid_visibility.is_err(),
        "visibility CHECK must reject corruption"
    );
}

#[tokio::test]
async fn explicit_character_rekey_preserves_record_provenance_and_refuses_merge() {
    let (_home, runtime) = runtime().await;
    let chloe = thread_id(30);
    let clanker = thread_id(31);
    seed_output(
        &runtime,
        named_scope(chloe, "chloe", project_a(), None, 100),
        "chloe",
    )
    .await;

    let receipt = runtime
        .memories()
        .rekey_character_memory(&canonical_id("chloe"), &canonical_id("cleo-next"))
        .await
        .unwrap();
    assert_eq!(receipt.updated_scopes, 1);
    assert_eq!(receipt.updated_outputs, 1);
    assert_eq!(
        receipt.affected_scopes,
        vec![
            named_selection("chloe", project_a()),
            named_selection("cleo-next", project_a()),
        ]
    );
    let rekeyed = runtime
        .memories()
        .select_scoped_memories(
            &MemorySelectionScope::Named {
                clanker_id: canonical_id("cleo-next"),
                project_key: project_a(),
            },
            10,
        )
        .await
        .unwrap();
    assert_eq!(rekeyed.len(), 1);
    assert_eq!(rekeyed[0].output.thread_id, chloe);
    assert_eq!(rekeyed[0].citation_path, Some(citation_path(chloe)));
    assert_eq!(rekeyed[0].output.source_updated_at, timestamp(100));

    seed_output(
        &runtime,
        named_scope(clanker, "clanker", project_a(), None, 100),
        "clanker",
    )
    .await;
    assert!(matches!(
        runtime
            .memories()
            .rekey_character_memory(&canonical_id("cleo-next"), &canonical_id("clanker"))
            .await,
        Err(MemoryScopeError::CharacterRekeyConflict { clanker_id })
            if clanker_id == canonical_id("clanker")
    ));
}

#[tokio::test]
async fn scoped_stage1_success_registers_output_and_phase2_job_atomically() {
    let (_home, runtime) = runtime().await;
    let source = thread_id(80);
    let now = Utc::now();
    seed_thread(&runtime, source, now).await;
    let scope = named_scope(
        source,
        "chloe",
        project_a(),
        /*parent*/ None,
        now.timestamp(),
    );
    let claim = runtime
        .memories()
        .try_claim_stage1_job(
            source,
            thread_id(999),
            now.timestamp(),
            /*lease_seconds*/ 3_600,
            /*max_running_jobs*/ 64,
        )
        .await
        .unwrap();
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        panic!("unexpected stage1 claim: {claim:?}");
    };
    assert!(
        runtime
            .memories()
            .mark_stage1_job_succeeded_scoped(
                &scope,
                &citation_path(source),
                ownership_token.as_str(),
                crate::Stage1MemoryPayload {
                    source_updated_at: now.timestamp(),
                    raw_memory: "raw",
                    rollout_summary: "summary",
                    rollout_slug: None,
                },
            )
            .await
            .unwrap()
    );

    assert_eq!(
        runtime.memories().memory_scope(source).await.unwrap(),
        Some(scope.clone())
    );
    let output_scope: (Option<String>, Option<String>, String, Option<String>) = sqlx::query_as(
        "SELECT clanker_id, project_key, visibility, citation_path FROM stage1_outputs WHERE thread_id = ?",
    )
    .bind(source.to_string())
    .fetch_one(runtime.memories().pool_for_tests())
    .await
    .unwrap();
    assert_eq!(
        output_scope,
        (
            Some("chloe".to_string()),
            Some(project_a().as_str().to_string()),
            "private_character".to_string(),
            Some(citation_path(source).as_str().to_string()),
        )
    );
    let jobs: Vec<(String, String)> = sqlx::query_as(
        "SELECT kind, job_key FROM jobs WHERE kind LIKE 'memory_consolidate_%' ORDER BY kind, job_key",
    )
    .fetch_all(runtime.memories().pool_for_tests())
    .await
    .unwrap();
    assert_eq!(
        jobs,
        vec![(
            "memory_consolidate_scoped".to_string(),
            scope.selection_scope().phase2_key(),
        )]
    );
}

#[tokio::test]
async fn scoped_stage1_no_output_registers_anonymous_scope_atomically() {
    let (_home, runtime) = runtime().await;
    let source = thread_id(801);
    let now = Utc::now();
    seed_thread(&runtime, source, now).await;
    let scope = anonymous_scope(source, project_a(), now.timestamp());
    let claim = runtime
        .memories()
        .try_claim_stage1_job(source, thread_id(999), now.timestamp(), 3_600, 64)
        .await
        .unwrap();
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        panic!("unexpected stage1 claim: {claim:?}");
    };

    assert!(
        !runtime
            .memories()
            .mark_stage1_job_succeeded_no_output_scoped(&scope, "wrong-owner")
            .await
            .unwrap()
    );
    assert_eq!(runtime.memories().memory_scope(source).await.unwrap(), None);
    assert!(
        runtime
            .memories()
            .mark_stage1_job_succeeded_no_output_scoped(&scope, ownership_token.as_str())
            .await
            .unwrap()
    );
    assert_eq!(
        runtime.memories().memory_scope(source).await.unwrap(),
        Some(scope)
    );
    let output_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs WHERE thread_id = ?")
            .bind(source.to_string())
            .fetch_one(runtime.memories().pool_for_tests())
            .await
            .unwrap();
    assert_eq!(output_count, 0);
}

#[tokio::test]
async fn scoped_phase2_success_uses_canonical_key_and_replaces_its_snapshot() {
    let (_home, runtime) = runtime().await;
    let now = Utc::now();
    let source = thread_id(81);
    let scope = named_scope(source, "chloe", project_a(), None, now.timestamp());
    seed_thread(&runtime, source, now).await;
    let claim = runtime
        .memories()
        .try_claim_stage1_job(source, thread_id(999), now.timestamp(), 3_600, 64)
        .await
        .unwrap();
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        panic!("unexpected stage1 claim: {claim:?}");
    };
    runtime
        .memories()
        .mark_stage1_job_succeeded_scoped(
            &scope,
            &citation_path(source),
            ownership_token.as_str(),
            crate::Stage1MemoryPayload {
                source_updated_at: now.timestamp(),
                raw_memory: "raw",
                rollout_summary: "summary",
                rollout_slug: None,
            },
        )
        .await
        .unwrap();
    let selection_scope = scope.selection_scope();
    let selected = runtime
        .memories()
        .select_scoped_phase2_inputs(&selection_scope, 10, 30)
        .await
        .unwrap();
    assert_eq!(selected.len(), 1);
    let claim = runtime
        .memories()
        .try_claim_selection_phase2_job(&selection_scope, thread_id(998), 3_600)
        .await
        .unwrap();
    let Phase2JobClaimOutcome::Claimed {
        ownership_token,
        input_watermark,
    } = claim
    else {
        panic!("unexpected phase2 claim: {claim:?}");
    };
    assert!(
        runtime
            .memories()
            .mark_selection_phase2_job_succeeded(
                &selection_scope,
                ownership_token.as_str(),
                input_watermark,
                input_watermark.max(now.timestamp()),
                selected.as_slice(),
            )
            .await
            .unwrap()
    );
    let snapshots: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT scope_key, thread_id, source_updated_at FROM phase2_scope_outputs ORDER BY scope_key, thread_id",
    )
    .fetch_all(runtime.memories().pool_for_tests())
    .await
    .unwrap();
    assert_eq!(
        snapshots,
        vec![(
            selection_scope.phase2_key(),
            source.to_string(),
            now.timestamp(),
        )]
    );
    assert!(
        runtime
            .memories()
            .select_scoped_phase2_inputs(&named_selection("chloe", project_b()), 10, 30)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn visibility_changes_enqueue_every_affected_scope_in_canonical_order() {
    let (_home, runtime) = runtime().await;
    let source = thread_id(82);
    seed_output(
        &runtime,
        named_scope(source, "clanker", project_a(), None, 100),
        "promoted",
    )
    .await;
    for scope in [
        named_scope(thread_id(83), "chloe", project_a(), None, 100),
        named_scope(thread_id(84), "orion", project_a(), None, 100),
        named_scope(thread_id(85), "chloe", project_b(), None, 100),
    ] {
        register_scope_only(&runtime, scope).await;
    }

    runtime
        .memories()
        .set_trusted_memory_visibility(source, MemoryVisibility::ProjectShared)
        .await
        .unwrap();
    let shared_keys: Vec<String> = sqlx::query_scalar(
        "SELECT job_key FROM jobs WHERE kind = 'memory_consolidate_scoped' ORDER BY job_key",
    )
    .fetch_all(runtime.memories().pool_for_tests())
    .await
    .unwrap();
    let mut expected_shared = vec![
        named_selection("chloe", project_a()).phase2_key(),
        named_selection("clanker", project_a()).phase2_key(),
        named_selection("orion", project_a()).phase2_key(),
    ];
    expected_shared.sort();
    assert_eq!(shared_keys, expected_shared);

    runtime
        .memories()
        .set_trusted_memory_visibility(source, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();
    let global_keys: Vec<String> = sqlx::query_scalar(
        "SELECT job_key FROM jobs WHERE kind = 'memory_consolidate_scoped' ORDER BY job_key",
    )
    .fetch_all(runtime.memories().pool_for_tests())
    .await
    .unwrap();
    let mut expected_global = vec![
        named_selection("chloe", project_a()).phase2_key(),
        named_selection("chloe", project_b()).phase2_key(),
        named_selection("clanker", project_a()).phase2_key(),
        named_selection("orion", project_a()).phase2_key(),
    ];
    expected_global.sort();
    assert_eq!(global_keys, expected_global);
}

#[tokio::test]
async fn scoped_phase2_selection_pages_past_startup_cap_and_disabled_candidates() {
    let (_home, runtime) = runtime().await;
    let now = Utc::now();
    let clanker_id = canonical_id("chloe");
    let project_key = project_a();
    let disabled_count = 16_u128;
    let total = 272_u128;
    let mut disabled_ids = Vec::new();

    for index in 0..total {
        let source = thread_id(2_000 + index);
        let scope = MemoryScope {
            thread_id: source,
            clanker_id: Some(clanker_id.clone()),
            project_key: project_key.clone(),
            parent_thread_id: None,
            recorded_at: now - Duration::seconds(index as i64),
        };
        seed_output(&runtime, scope, "paged").await;
        if index < disabled_count {
            disabled_ids.push(source);
            runtime
                .set_thread_memory_mode(source, "disabled")
                .await
                .unwrap();
        }
    }

    let selection_scope = MemorySelectionScope::Named {
        clanker_id,
        project_key,
    };
    let configured = runtime
        .memories()
        .select_scoped_phase2_inputs(&selection_scope, 137, 30)
        .await
        .unwrap();
    assert_eq!(configured.len(), 137);
    let default_capacity = runtime
        .memories()
        .select_scoped_phase2_inputs(&selection_scope, 256, 30)
        .await
        .unwrap();
    assert_eq!(default_capacity.len(), 256);
    assert!(
        default_capacity
            .iter()
            .all(|record| !disabled_ids.contains(&record.output.thread_id))
    );
    assert!(
        default_capacity
            .windows(2)
            .all(|pair| pair[0].output.thread_id.to_string() < pair[1].output.thread_id.to_string())
    );
}

#[tokio::test]
async fn anonymous_scoped_phase2_selection_preserves_legacy_paging_and_excludes_named() {
    let (_home, runtime) = runtime().await;
    let now = Utc::now();
    for index in 0..140_u128 {
        seed_output(
            &runtime,
            anonymous_scope(
                thread_id(3_000 + index),
                project_a(),
                (now - Duration::seconds(index as i64)).timestamp(),
            ),
            "anonymous",
        )
        .await;
    }
    let named = thread_id(3_500);
    seed_output(
        &runtime,
        named_scope(named, "chloe", project_a(), None, now.timestamp()),
        "named",
    )
    .await;

    let legacy = runtime
        .memories()
        .get_phase2_input_selection(128, 30)
        .await
        .unwrap();
    let scoped = runtime
        .memories()
        .select_scoped_phase2_inputs(&MemorySelectionScope::Anonymous, 128, 30)
        .await
        .unwrap();
    assert_eq!(
        scoped
            .iter()
            .map(|record| record.output.thread_id)
            .collect::<Vec<_>>(),
        legacy
            .iter()
            .map(|output| output.thread_id)
            .collect::<Vec<_>>()
    );
    assert!(!legacy.iter().any(|output| output.thread_id == named));
}

#[tokio::test]
async fn retention_preserves_outputs_referenced_only_by_named_snapshots() {
    let (_home, runtime) = runtime().await;
    let source = thread_id(3_600);
    let old = Utc::now() - Duration::days(60);
    let scope = named_scope(source, "chloe", project_a(), None, old.timestamp());
    seed_output(&runtime, scope.clone(), "selected").await;
    let MemorySelectionScope::Named {
        clanker_id,
        project_key,
    } = scope.selection_scope()
    else {
        unreachable!();
    };
    sqlx::query(
        "INSERT INTO phase2_scope_outputs (scope_key, clanker_id, project_key, thread_id, source_updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(scope.selection_scope().phase2_key())
    .bind(clanker_id.as_str())
    .bind(project_key.as_str())
    .bind(source.to_string())
    .bind(old.timestamp())
    .execute(runtime.memories().pool_for_tests())
    .await
    .unwrap();

    assert_eq!(
        runtime
            .memories()
            .prune_stage1_outputs_for_retention(30, 100)
            .await
            .unwrap(),
        0
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs WHERE thread_id = ?")
        .bind(source.to_string())
        .fetch_one(runtime.memories().pool_for_tests())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn stale_named_and_anonymous_phase2_completions_preserve_snapshots_and_reclaim_immediately() {
    for (index, selection_scope) in [
        named_selection("chloe", project_a()),
        MemorySelectionScope::Anonymous,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, runtime) = runtime().await;
        let source = thread_id(3_700 + index as u128);
        let now = Utc::now();
        let scope = match &selection_scope {
            MemorySelectionScope::Named {
                clanker_id,
                project_key,
            } => MemoryScope {
                thread_id: source,
                clanker_id: Some(clanker_id.clone()),
                project_key: project_key.clone(),
                parent_thread_id: None,
                recorded_at: now,
            },
            MemorySelectionScope::Anonymous => {
                anonymous_scope(source, project_a(), now.timestamp())
            }
        };
        seed_output(&runtime, scope, "snapshot").await;
        let selected = runtime
            .memories()
            .select_scoped_phase2_inputs(&selection_scope, 10, 30)
            .await
            .unwrap();
        let claim = runtime
            .memories()
            .try_claim_selection_phase2_job(&selection_scope, thread_id(3_999), 3_600)
            .await
            .unwrap();
        let Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } = claim
        else {
            panic!("unexpected phase2 claim: {claim:?}");
        };
        assert!(
            runtime
                .memories()
                .mark_selection_phase2_job_succeeded(
                    &selection_scope,
                    ownership_token.as_str(),
                    input_watermark,
                    input_watermark,
                    selected.as_slice(),
                )
                .await
                .unwrap()
        );

        let next_claim = runtime
            .memories()
            .try_claim_selection_phase2_job(&selection_scope, thread_id(3_998), 3_600)
            .await
            .unwrap();
        assert!(matches!(next_claim, Phase2JobClaimOutcome::SkippedCooldown));
        let kind = if matches!(selection_scope, MemorySelectionScope::Anonymous) {
            "memory_consolidate_global"
        } else {
            "memory_consolidate_scoped"
        };
        let key = if matches!(selection_scope, MemorySelectionScope::Anonymous) {
            "global".to_string()
        } else {
            selection_scope.phase2_key()
        };
        sqlx::query("UPDATE jobs SET finished_at = 0 WHERE kind = ? AND job_key = ?")
            .bind(kind)
            .bind(key.as_str())
            .execute(runtime.memories().pool_for_tests())
            .await
            .unwrap();
        let running = runtime
            .memories()
            .try_claim_selection_phase2_job(&selection_scope, thread_id(3_997), 3_600)
            .await
            .unwrap();
        let Phase2JobClaimOutcome::Claimed {
            ownership_token: stale_token,
            input_watermark: claimed_watermark,
        } = running
        else {
            panic!("unexpected second claim: {running:?}");
        };
        if matches!(selection_scope, MemorySelectionScope::Anonymous) {
            runtime
                .memories()
                .enqueue_global_consolidation(claimed_watermark + 1)
                .await
                .unwrap();
        } else {
            runtime
                .memories()
                .set_trusted_memory_visibility(source, MemoryVisibility::ProjectShared)
                .await
                .unwrap();
        }

        assert!(
            !runtime
                .memories()
                .mark_selection_phase2_job_succeeded(
                    &selection_scope,
                    stale_token.as_str(),
                    claimed_watermark,
                    claimed_watermark,
                    &[],
                )
                .await
                .unwrap()
        );
        let snapshot_count: i64 = if matches!(selection_scope, MemorySelectionScope::Anonymous) {
            sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs WHERE selected_for_phase2 = 1")
                .fetch_one(runtime.memories().pool_for_tests())
                .await
                .unwrap()
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM phase2_scope_outputs WHERE scope_key = ?")
                .bind(selection_scope.phase2_key())
                .fetch_one(runtime.memories().pool_for_tests())
                .await
                .unwrap()
        };
        assert_eq!(snapshot_count, 1);
        let reclaimed = runtime
            .memories()
            .try_claim_selection_phase2_job(&selection_scope, thread_id(3_996), 3_600)
            .await
            .unwrap();
        let Phase2JobClaimOutcome::Claimed {
            input_watermark: reclaimed_watermark,
            ..
        } = reclaimed
        else {
            panic!("stale completion should be immediately reclaimable: {reclaimed:?}");
        };
        assert!(reclaimed_watermark > claimed_watermark);
    }
}

#[tokio::test]
async fn pollution_enqueues_snapshot_consumers_and_private_shared_global_visibility_fanout() {
    let (_home, runtime) = runtime().await;
    let private = thread_id(4_000);
    let shared = thread_id(4_001);
    let global = thread_id(4_002);
    seed_output(
        &runtime,
        named_scope(private, "chloe", project_a(), None, 100),
        "private",
    )
    .await;
    seed_output(
        &runtime,
        named_scope(shared, "clanker", project_a(), None, 101),
        "shared",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(shared, MemoryVisibility::ProjectShared)
        .await
        .unwrap();
    seed_output(
        &runtime,
        named_scope(global, "orion", project_b(), None, 102),
        "global",
    )
    .await;
    runtime
        .memories()
        .set_trusted_memory_visibility(global, MemoryVisibility::GlobalUserPreference)
        .await
        .unwrap();
    register_scope_only(
        &runtime,
        named_scope(thread_id(4_010), "orion", project_a(), None, 100),
    )
    .await;
    register_scope_only(
        &runtime,
        named_scope(thread_id(4_011), "chloe", project_b(), None, 100),
    )
    .await;

    let historical_consumer = named_selection("chloe", project_b());
    insert_named_snapshot(&runtime, &historical_consumer, private, 100).await;
    insert_named_snapshot(&runtime, &historical_consumer, shared, 101).await;
    sqlx::query("DELETE FROM jobs WHERE kind LIKE 'memory_consolidate_%'")
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();

    runtime
        .memories()
        .mark_thread_memory_mode_polluted(private)
        .await
        .unwrap();
    assert_eq!(
        scoped_job_keys(&runtime).await,
        sorted_scope_keys([
            named_selection("chloe", project_a()),
            historical_consumer.clone(),
        ])
    );
    sqlx::query("DELETE FROM jobs WHERE kind LIKE 'memory_consolidate_%'")
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();

    runtime
        .memories()
        .mark_thread_memory_mode_polluted(shared)
        .await
        .unwrap();
    assert_eq!(
        scoped_job_keys(&runtime).await,
        sorted_scope_keys([
            named_selection("chloe", project_a()),
            named_selection("clanker", project_a()),
            named_selection("orion", project_a()),
            historical_consumer.clone(),
        ])
    );
    sqlx::query("DELETE FROM jobs WHERE kind LIKE 'memory_consolidate_%'")
        .execute(runtime.memories().pool_for_tests())
        .await
        .unwrap();

    runtime
        .memories()
        .mark_thread_memory_mode_polluted(global)
        .await
        .unwrap();
    assert_eq!(
        scoped_job_keys(&runtime).await,
        sorted_scope_keys([
            named_selection("chloe", project_a()),
            named_selection("clanker", project_a()),
            named_selection("orion", project_a()),
            historical_consumer,
            named_selection("orion", project_b()),
        ])
    );
}

async fn runtime() -> (TempDir, Arc<StateRuntime>) {
    let home = tempfile::tempdir().expect("tempdir");
    let runtime = StateRuntime::init(home.path().to_path_buf(), "mock_provider".to_string())
        .await
        .expect("state runtime should initialize");
    (home, runtime)
}

async fn seed_output(runtime: &StateRuntime, scope: MemoryScope, body: &str) {
    let source_thread_id = seed_unbound_output(runtime, scope, body).await;
    let citation_path = citation_path(source_thread_id);
    runtime
        .memories()
        .bind_registered_scope_to_stage1_output(source_thread_id, Some(&citation_path))
        .await
        .expect("output scope should bind");
}

async fn seed_unbound_output(runtime: &StateRuntime, scope: MemoryScope, body: &str) -> ThreadId {
    let source_thread_id = scope.thread_id;
    seed_thread(runtime, source_thread_id, scope.recorded_at).await;
    runtime
        .memories()
        .register_memory_scope(&scope)
        .await
        .expect("scope should register");
    let claim = runtime
        .memories()
        .try_claim_stage1_job(
            source_thread_id,
            thread_id(999),
            scope.recorded_at.timestamp(),
            /*lease_seconds*/ 3_600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("stage1 job should be claimable");
    let Stage1JobClaimOutcome::Claimed { ownership_token } = claim else {
        panic!("unexpected claim: {claim:?}");
    };
    runtime
        .memories()
        .mark_stage1_job_succeeded(
            source_thread_id,
            ownership_token.as_str(),
            scope.recorded_at.timestamp(),
            body,
            body,
            /*rollout_slug*/ None,
        )
        .await
        .expect("stage1 output should persist");
    source_thread_id
}

async fn register_scope_only(runtime: &StateRuntime, scope: MemoryScope) {
    runtime
        .memories()
        .register_memory_scope(&scope)
        .await
        .expect("scope should register");
}

async fn insert_named_snapshot(
    runtime: &StateRuntime,
    scope: &MemorySelectionScope,
    source_thread_id: ThreadId,
    source_updated_at: i64,
) {
    let MemorySelectionScope::Named {
        clanker_id,
        project_key,
    } = scope
    else {
        panic!("snapshot helper requires a named scope");
    };
    sqlx::query(
        "INSERT INTO phase2_scope_outputs (scope_key, clanker_id, project_key, thread_id, source_updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(scope.phase2_key())
    .bind(clanker_id.as_str())
    .bind(project_key.as_str())
    .bind(source_thread_id.to_string())
    .bind(source_updated_at)
    .execute(runtime.memories().pool_for_tests())
    .await
    .expect("insert named snapshot");
}

async fn scoped_job_keys(runtime: &StateRuntime) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT job_key FROM jobs WHERE kind = 'memory_consolidate_scoped' ORDER BY job_key",
    )
    .fetch_all(runtime.memories().pool_for_tests())
    .await
    .expect("load scoped job keys")
}

fn sorted_scope_keys<const N: usize>(scopes: [MemorySelectionScope; N]) -> Vec<String> {
    let mut keys = scopes
        .into_iter()
        .map(|scope| scope.phase2_key())
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

async fn seed_thread(runtime: &StateRuntime, thread_id: ThreadId, updated_at: DateTime<Utc>) {
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        format!("/tmp/{thread_id}.jsonl").into(),
        updated_at,
        SessionSource::Cli,
    );
    builder.updated_at = Some(updated_at);
    builder.cwd = "/workspace".into();
    runtime
        .upsert_thread(&builder.build("mock_provider"))
        .await
        .expect("thread metadata should persist");
}

fn named_scope(
    thread_id: ThreadId,
    clanker_id: &str,
    project_key: MemoryProjectKey,
    parent_thread_id: Option<ThreadId>,
    recorded_at: i64,
) -> MemoryScope {
    MemoryScope {
        thread_id,
        clanker_id: Some(canonical_id(clanker_id)),
        project_key,
        parent_thread_id,
        recorded_at: timestamp(recorded_at),
    }
}

fn anonymous_scope(
    thread_id: ThreadId,
    project_key: MemoryProjectKey,
    recorded_at: i64,
) -> MemoryScope {
    MemoryScope {
        thread_id,
        clanker_id: None,
        project_key,
        parent_thread_id: None,
        recorded_at: timestamp(recorded_at),
    }
}

fn canonical_id(value: &str) -> CanonicalClankerId {
    let (_home, catalog) = catalog_with_character(value, &[]);
    CanonicalClankerId::resolve_exact(&catalog, value).expect("test id should be canonical")
}

fn catalog_with_character(id: &str, aliases: &[&str]) -> (TempDir, CharacterCatalog) {
    let home = tempfile::tempdir().expect("character catalog home");
    let package = home.path().join("characters").join(id);
    let avatar = package.join("avatar");
    fs::create_dir_all(&avatar).expect("create character package");
    fs::write(
        package.join("character.json"),
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "id": id,
            "displayName": id,
            "aliases": aliases,
            "avatar": "avatar/avatar.json"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        avatar.join("avatar.json"),
        r#"{
            "renderMode": "ansi-half-block",
            "spritesheetPath": "sheet.ppm",
            "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1}
        }"#,
    )
    .unwrap();
    write_ppm(&avatar.join("sheet.ppm"));
    let catalog = CharacterCatalog::load(home.path());
    (home, catalog)
}

fn write_ppm(path: &Path) {
    let pixels = "0 0 0\n".repeat(24 * 24);
    fs::write(path, format!("P3\n24 24\n255\n{pixels}")).unwrap();
}

fn project_a() -> MemoryProjectKey {
    MemoryProjectKey::from_git_origin("git@github.com:example/project-a.git")
        .expect("test origin should canonicalize")
}

fn project_b() -> MemoryProjectKey {
    MemoryProjectKey::from_git_origin("https://github.com/example/project-b.git")
        .expect("test origin should canonicalize")
}

fn named_selection(clanker_id: &str, project_key: MemoryProjectKey) -> MemorySelectionScope {
    MemorySelectionScope::Named {
        clanker_id: canonical_id(clanker_id),
        project_key,
    }
}

fn thread_id(value: u128) -> ThreadId {
    let uuid = uuid::Uuid::from_u128(value);
    ThreadId::from_string(uuid.to_string().as_str()).expect("test thread id should be valid")
}

fn citation_path(thread_id: ThreadId) -> MemoryCitationPath {
    MemoryCitationPath::new(format!("rollout_summaries/{thread_id}.md")).unwrap()
}

fn timestamp(value: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(value, 0).expect("test timestamp should be valid")
}
