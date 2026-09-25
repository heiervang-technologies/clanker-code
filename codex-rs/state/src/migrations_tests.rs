use std::borrow::Cow;
use std::time::Duration;

use sqlx::Connection;
use sqlx::Row;
use sqlx::SqliteConnection;
use sqlx::migrate::Migration;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqliteJournalMode;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

use super::MEMORIES_MIGRATOR;
use super::STATE_MIGRATOR;
use super::repair_clanker_memories_migration_versions;
use super::repair_legacy_recency_migration_version;

fn migrator_through(version: i64) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            STATE_MIGRATOR
                .migrations
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        ),
        ignore_missing: STATE_MIGRATOR.ignore_missing,
        locking: STATE_MIGRATOR.locking,
        table_name: STATE_MIGRATOR.table_name.clone(),
        create_schemas: STATE_MIGRATOR.create_schemas.clone(),
        no_tx: STATE_MIGRATOR.no_tx,
    }
}

fn memories_migrator_through(version: i64) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            MEMORIES_MIGRATOR
                .migrations
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        ),
        ignore_missing: MEMORIES_MIGRATOR.ignore_missing,
        locking: MEMORIES_MIGRATOR.locking,
        table_name: MEMORIES_MIGRATOR.table_name.clone(),
        create_schemas: MEMORIES_MIGRATOR.create_schemas.clone(),
        no_tx: MEMORIES_MIGRATOR.no_tx,
    }
}

#[tokio::test]
async fn character_memory_migration_preserves_legacy_rows_as_anonymous() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");
    memories_migrator_through(/*version*/ 1)
        .run(&pool)
        .await
        .expect("legacy memories migration should apply");
    sqlx::query(
        r#"
INSERT INTO stage1_outputs (
    thread_id,
    source_updated_at,
    raw_memory,
    rollout_summary,
    generated_at
) VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .bind(100_i64)
    .bind("legacy raw")
    .bind("legacy summary")
    .bind(101_i64)
    .execute(&pool)
    .await
    .expect("legacy memory row should insert");

    MEMORIES_MIGRATOR
        .run(&pool)
        .await
        .expect("character memory migration should apply");
    let row = sqlx::query(
        "SELECT clanker_id, project_key, visibility FROM stage1_outputs WHERE thread_id = ?",
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .fetch_one(&pool)
    .await
    .expect("legacy row should remain");
    assert_eq!(
        row.try_get::<Option<String>, _>("clanker_id").unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("project_key").unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<String, _>("visibility").unwrap(),
        "anonymous_legacy"
    );
    let scope_table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'thread_memory_scopes'",
    )
    .fetch_one(&pool)
    .await
    .expect("scope table lookup should work");
    assert_eq!(scope_table_exists, 1);
    let invalid_visibility =
        sqlx::query("UPDATE stage1_outputs SET visibility = 'untrusted' WHERE thread_id = ?")
            .bind("00000000-0000-0000-0000-000000000001")
            .execute(&pool)
            .await;
    assert!(
        invalid_visibility.is_err(),
        "visibility CHECK should reject unknown values"
    );
}

#[tokio::test]
async fn scoped_phase2_migration_preserves_character_memory_and_adds_baselines() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");
    memories_migrator_through(/*version*/ 1001)
        .run(&pool)
        .await
        .expect("character memory migration should apply");
    sqlx::query(
        "INSERT INTO stage1_outputs (thread_id, source_updated_at, raw_memory, rollout_summary, generated_at, visibility) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .bind(100_i64)
    .bind("legacy raw")
    .bind("legacy summary")
    .bind(101_i64)
    .bind("anonymous_legacy")
    .execute(&pool)
    .await
    .expect("pre-scoped row should insert");

    MEMORIES_MIGRATOR
        .run(&pool)
        .await
        .expect("scoped phase2 migration should apply");
    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs")
        .fetch_one(&pool)
        .await
        .expect("stage1 output count should load");
    let baseline_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'phase2_scope_outputs'",
    )
    .fetch_one(&pool)
    .await
    .expect("phase2 baseline table lookup should work");
    assert_eq!(retained, 1);
    assert_eq!(baseline_table, 1);
}

#[tokio::test]
async fn recency_migration_backfills_and_seeds_old_binary_inserts() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");
    migrator_through(/*version*/ 37)
        .run(&pool)
        .await
        .expect("pre-recency migrations should apply");

    sqlx::query(
        r#"
INSERT INTO threads (
    id,
    rollout_path,
    created_at,
    updated_at,
    created_at_ms,
    updated_at_ms,
    source,
    model_provider,
    cwd,
    title,
    sandbox_policy,
    approval_mode
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .bind("/tmp/first.jsonl")
    .bind(1_700_000_000_i64)
    .bind(1_700_000_100_i64)
    .bind(1_700_000_000_123_i64)
    .bind(1_700_000_100_456_i64)
    .bind("cli")
    .bind("openai")
    .bind("/tmp")
    .bind("")
    .bind("read-only")
    .bind("on-request")
    .execute(&pool)
    .await
    .expect("legacy row should insert");

    STATE_MIGRATOR
        .run(&pool)
        .await
        .expect("recency migration should apply");

    let backfilled = sqlx::query(
        "SELECT updated_at, updated_at_ms, recency_at, recency_at_ms FROM threads WHERE id = ?",
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .fetch_one(&pool)
    .await
    .expect("backfilled row should load");
    assert_eq!(backfilled.get::<i64, _>("recency_at"), 1_700_000_100);
    assert_eq!(backfilled.get::<i64, _>("recency_at_ms"), 1_700_000_100_456);

    sqlx::query(
        r#"
INSERT INTO threads (
    id,
    rollout_path,
    created_at,
    updated_at,
    created_at_ms,
    updated_at_ms,
    source,
    model_provider,
    cwd,
    title,
    sandbox_policy,
    approval_mode
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("00000000-0000-0000-0000-000000000002")
    .bind("/tmp/second.jsonl")
    .bind(1_700_000_200_i64)
    .bind(1_700_000_300_i64)
    .bind(1_700_000_200_123_i64)
    .bind(1_700_000_300_456_i64)
    .bind("cli")
    .bind("openai")
    .bind("/tmp")
    .bind("")
    .bind("read-only")
    .bind("on-request")
    .execute(&pool)
    .await
    .expect("old-binary row should insert");

    let seeded = sqlx::query("SELECT recency_at, recency_at_ms FROM threads WHERE id = ?")
        .bind("00000000-0000-0000-0000-000000000002")
        .fetch_one(&pool)
        .await
        .expect("old-binary row should load");
    assert_eq!(seeded.get::<i64, _>("recency_at"), 1_700_000_300);
    assert_eq!(seeded.get::<i64, _>("recency_at_ms"), 1_700_000_300_456);
}

#[tokio::test]
async fn repairs_recency_migration_that_was_applied_as_version_38() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");
    migrator_through(/*version*/ 37)
        .run(&pool)
        .await
        .expect("pre-recency migrations should apply");

    let recency_migration = STATE_MIGRATOR
        .migrations
        .iter()
        .find(|migration| migration.version == 39)
        .expect("recency migration should exist");
    let mut legacy_migrations = STATE_MIGRATOR
        .migrations
        .iter()
        .filter(|migration| migration.version <= 37)
        .cloned()
        .collect::<Vec<_>>();
    legacy_migrations.push(Migration::new(
        38,
        recency_migration.description.clone(),
        recency_migration.migration_type,
        recency_migration.sql.clone(),
        recency_migration.no_tx,
    ));
    let legacy_recency_migrator = Migrator::with_migrations(legacy_migrations);
    legacy_recency_migrator
        .run(&pool)
        .await
        .expect("legacy recency migration should apply as version 38");

    repair_legacy_recency_migration_version(&pool, &STATE_MIGRATOR)
        .await
        .expect("legacy migration history should be repaired");
    STATE_MIGRATOR
        .run(&pool)
        .await
        .expect("current migrations should apply after repair");

    let applied = sqlx::query(
        "SELECT version, checksum FROM _sqlx_migrations WHERE version >= 38 ORDER BY version",
    )
    .fetch_all(&pool)
    .await
    .expect("applied migrations should load")
    .into_iter()
    .map(|row| {
        (
            row.get::<i64, _>("version"),
            row.get::<Vec<u8>, _>("checksum"),
        )
    })
    .collect::<Vec<_>>();
    let expected = STATE_MIGRATOR
        .migrations
        .iter()
        .filter(|migration| migration.version >= 38)
        .map(|migration| (migration.version, migration.checksum.to_vec()))
        .collect::<Vec<_>>();
    assert_eq!(applied, expected);
}

#[tokio::test]
async fn repair_recency_migration_succeeds_while_another_connection_holds_writer_slot() {
    let database_path = std::env::temp_dir().join(format!(
        "codex-state-migrations-test-{}.sqlite",
        Uuid::new_v4()
    ));
    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(100));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .expect("database should open");
    STATE_MIGRATOR
        .run(&pool)
        .await
        .expect("current migrations should apply");
    let mut write_connection = SqliteConnection::connect_with(&options)
        .await
        .expect("write connection should open");
    let write_transaction = write_connection
        .begin_with("BEGIN IMMEDIATE")
        .await
        .expect("write transaction should acquire the writer slot");

    let repair_result = repair_legacy_recency_migration_version(&pool, &STATE_MIGRATOR).await;

    write_transaction
        .rollback()
        .await
        .expect("write transaction should roll back");
    drop(write_connection);
    pool.close().await;
    let _ = tokio::fs::remove_file(database_path).await;
    repair_result.expect("current migration history should not need the writer slot");
}

#[tokio::test]
async fn repairs_clanker_memories_migrations_applied_as_versions_2_and_3() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");

    // Recreate the history left by older Clanker builds: character memory
    // scope as version 2 and scoped phase2 as version 3.
    let migration = |version: i64| {
        MEMORIES_MIGRATOR
            .migrations
            .iter()
            .find(|migration| migration.version == version)
            .expect("memories migration should exist")
    };
    let mut legacy_migrations = vec![migration(1).clone()];
    for (legacy_version, version) in [(2, 1001), (3, 1002)] {
        let current = migration(version);
        legacy_migrations.push(Migration::new(
            legacy_version,
            current.description.clone(),
            current.migration_type,
            current.sql.clone(),
            current.no_tx,
        ));
    }
    Migrator::with_migrations(legacy_migrations)
        .run(&pool)
        .await
        .expect("legacy Clanker memories migrations should apply");

    repair_clanker_memories_migration_versions(&pool, &MEMORIES_MIGRATOR)
        .await
        .expect("legacy memories migration history should be repaired");
    MEMORIES_MIGRATOR
        .run(&pool)
        .await
        .expect("current memories migrations should apply after repair");

    let applied = sqlx::query("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .expect("applied migrations should load")
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("version"),
                row.get::<Vec<u8>, _>("checksum"),
            )
        })
        .collect::<Vec<_>>();
    let expected = MEMORIES_MIGRATOR
        .migrations
        .iter()
        .map(|migration| (migration.version, migration.checksum.to_vec()))
        .collect::<Vec<_>>();
    assert_eq!(applied, expected);
    let consolidation_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM consolidation_progress")
        .fetch_one(&pool)
        .await
        .expect("upstream consolidation table should exist");
    assert_eq!(consolidation_rows, 1);
}

#[tokio::test]
async fn clanker_memories_repair_leaves_upstream_version_2_alone() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory database should open");
    // Upstream Codex stamps only 0001 and its own 0002.
    memories_migrator_through(/*version*/ 2)
        .run(&pool)
        .await
        .expect("upstream memories migrations should apply");

    repair_clanker_memories_migration_versions(&pool, &MEMORIES_MIGRATOR)
        .await
        .expect("repair should be a no-op for upstream history");
    MEMORIES_MIGRATOR
        .run(&pool)
        .await
        .expect("Clanker migrations should apply on top of upstream history");

    let versions =
        sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("applied versions should load");
    assert_eq!(versions, vec![1, 2, 1001, 1002]);
}
