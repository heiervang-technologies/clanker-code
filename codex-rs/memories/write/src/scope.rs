use codex_character::CharacterCatalog;
use codex_state::CanonicalClankerId;
use codex_state::MemoryCitationPath;
use codex_state::MemoryProjectKey;
use codex_state::MemoryScope;
use codex_state::StateRuntime;
use codex_state::ThreadMetadata;
use std::ffi::OsString;
use std::path::Path;

pub(crate) async fn current_thread_scope(
    db: &StateRuntime,
    thread_id: codex_protocol::ThreadId,
    codex_home: &Path,
    current_cwd: &Path,
    parent_thread_id: Option<codex_protocol::ThreadId>,
    launcher_identity: Option<OsString>,
) -> anyhow::Result<MemoryScope> {
    current_thread_scope_with_launcher_identity(
        db,
        thread_id,
        codex_home,
        current_cwd,
        parent_thread_id,
        launcher_identity,
    )
    .await
}

async fn current_thread_scope_with_launcher_identity(
    db: &StateRuntime,
    thread_id: codex_protocol::ThreadId,
    codex_home: &Path,
    current_cwd: &Path,
    parent_thread_id: Option<codex_protocol::ThreadId>,
    launcher_identity: Option<OsString>,
) -> anyhow::Result<MemoryScope> {
    if let Some(scope) = db.memories().memory_scope(thread_id).await? {
        return Ok(scope);
    }
    let launcher_id = launcher_identity
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("CLANKER_ID is not valid UTF-8"))
        })
        .transpose()?;
    current_thread_scope_with_identity(
        db,
        thread_id,
        codex_home,
        current_cwd,
        parent_thread_id,
        launcher_id.as_deref(),
    )
    .await
}

async fn current_thread_scope_with_identity(
    db: &StateRuntime,
    thread_id: codex_protocol::ThreadId,
    codex_home: &Path,
    current_cwd: &Path,
    parent_thread_id: Option<codex_protocol::ThreadId>,
    launcher_id: Option<&str>,
) -> anyhow::Result<MemoryScope> {
    if let Some(scope) = db.memories().memory_scope(thread_id).await? {
        return Ok(scope);
    }
    let thread = db.get_thread(thread_id).await?;
    let clanker_id = match launcher_id {
        Some(value) => Some(CanonicalClankerId::resolve_exact(
            &CharacterCatalog::load(codex_home),
            value,
        )?),
        None => None,
    };
    let scope = MemoryScope {
        thread_id,
        clanker_id,
        project_key: project_key_from_live_cwd(current_cwd).await?,
        parent_thread_id,
        recorded_at: thread.map_or_else(chrono::Utc::now, |thread| thread.created_at),
    };
    db.memories().register_memory_scope(&scope).await?;
    Ok(scope)
}

/// Resolves a claimed rollout only from its persisted state. In particular,
/// this path must not inspect the current launcher identity or cwd.
pub(crate) async fn claimed_thread_scope(
    db: &StateRuntime,
    thread: &ThreadMetadata,
) -> anyhow::Result<MemoryScope> {
    if let Some(scope) = db.memories().memory_scope(thread.id).await? {
        return Ok(scope);
    }
    Ok(MemoryScope {
        thread_id: thread.id,
        clanker_id: None,
        project_key: project_key_from_thread(thread)?,
        parent_thread_id: None,
        recorded_at: thread.created_at,
    })
}

pub(crate) fn citation_path_for_thread(
    thread_id: codex_protocol::ThreadId,
) -> anyhow::Result<MemoryCitationPath> {
    Ok(MemoryCitationPath::new(format!(
        "rollout_summaries/{thread_id}.md"
    ))?)
}

fn project_key_from_thread(thread: &ThreadMetadata) -> anyhow::Result<MemoryProjectKey> {
    match thread.git_origin_url.as_deref() {
        Some(origin) => Ok(MemoryProjectKey::from_git_origin(origin)?),
        None => Ok(MemoryProjectKey::from_canonical_path(&thread.cwd)?),
    }
}

async fn project_key_from_live_cwd(cwd: &Path) -> anyhow::Result<MemoryProjectKey> {
    let git_info = codex_git_utils::collect_git_info(cwd).await;
    match git_info
        .as_ref()
        .and_then(|git_info| git_info.repository_url.as_deref())
    {
        Some(origin) => Ok(MemoryProjectKey::from_git_origin(origin)?),
        None => Ok(MemoryProjectKey::from_canonical_path(cwd)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SessionSource;
    use codex_state::MemoryScopeRegistration;
    use codex_state::ThreadMetadataBuilder;
    use tempfile::tempdir;

    #[tokio::test]
    async fn existing_anonymous_scope_is_not_adopted_by_later_named_launch() {
        let home = tempdir().unwrap();
        write_character(home.path(), "chloe", &["cleo"]);
        let runtime = StateRuntime::init(home.path().to_path_buf(), "mock".to_string())
            .await
            .unwrap();
        let thread_id = ThreadId::new();
        seed_thread(&runtime, thread_id, "/persisted/project", None).await;
        let anonymous = current_thread_scope_with_identity(
            runtime.as_ref(),
            thread_id,
            home.path(),
            Path::new("/persisted/project"),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(anonymous.clanker_id.is_none());
        let replay = current_thread_scope_with_identity(
            runtime.as_ref(),
            thread_id,
            home.path(),
            Path::new("/different/live/project"),
            None,
            Some("chloe"),
        )
        .await
        .unwrap();
        assert_eq!(replay.clanker_id, anonymous.clanker_id);
        assert_eq!(replay.project_key, anonymous.project_key);
        assert_eq!(replay.parent_thread_id, anonymous.parent_thread_id);
        assert_eq!(
            runtime
                .memories()
                .register_memory_scope(&replay)
                .await
                .unwrap(),
            MemoryScopeRegistration::ReplayedExact
        );
    }

    #[tokio::test]
    async fn existing_scope_wins_before_invalid_launcher_identity_is_decoded_or_resolved() {
        let home = tempdir().unwrap();
        let runtime = StateRuntime::init(home.path().to_path_buf(), "mock".to_string())
            .await
            .unwrap();
        let thread_id = ThreadId::new();
        seed_thread(&runtime, thread_id, "/persisted/project", None).await;
        let stored = current_thread_scope_with_identity(
            runtime.as_ref(),
            thread_id,
            home.path(),
            Path::new("/persisted/project"),
            None,
            None,
        )
        .await
        .unwrap();

        for launcher in [
            None,
            Some(OsString::from("Missing")),
            Some(OsString::from("not-an-id")),
        ] {
            let replay = current_thread_scope_with_launcher_identity(
                runtime.as_ref(),
                thread_id,
                home.path(),
                Path::new("/different/project"),
                Some(ThreadId::new()),
                launcher,
            )
            .await
            .unwrap();
            assert_eq!(replay.thread_id, stored.thread_id);
            assert_eq!(replay.clanker_id, stored.clanker_id);
            assert_eq!(replay.project_key, stored.project_key);
            assert_eq!(replay.parent_thread_id, stored.parent_thread_id);
            assert_eq!(
                replay.recorded_at.timestamp(),
                stored.recorded_at.timestamp()
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let replay = current_thread_scope_with_launcher_identity(
                runtime.as_ref(),
                thread_id,
                home.path(),
                Path::new("/different/project"),
                Some(ThreadId::new()),
                Some(OsString::from_vec(vec![0xff])),
            )
            .await
            .unwrap();
            assert_eq!(replay.project_key, stored.project_key);
        }
    }

    #[tokio::test]
    async fn current_named_scope_requires_exact_canonical_transport() {
        let home = tempdir().unwrap();
        write_character(home.path(), "chloe", &["cleo"]);
        for (offset, rejected) in ["Chloe", "cleo", "missing"].into_iter().enumerate() {
            let runtime = StateRuntime::init(
                home.path().join(format!("state-{offset}")),
                "mock".to_string(),
            )
            .await
            .unwrap();
            let thread_id = ThreadId::new();
            seed_thread(&runtime, thread_id, "/persisted/project", None).await;
            assert!(
                current_thread_scope_with_identity(
                    runtime.as_ref(),
                    thread_id,
                    home.path(),
                    Path::new("/persisted/project"),
                    None,
                    Some(rejected),
                )
                .await
                .is_err(),
                "{rejected}"
            );
        }
    }

    #[tokio::test]
    async fn current_scope_registers_before_thread_metadata_is_persisted() {
        let home = tempdir().unwrap();
        let project = tempdir().unwrap();
        let project_path = project.path().canonicalize().unwrap();
        let runtime = StateRuntime::init(home.path().to_path_buf(), "mock".to_string())
            .await
            .unwrap();
        let thread_id = ThreadId::new();
        let parent_thread_id = ThreadId::new();

        let scope = current_thread_scope_with_identity(
            runtime.as_ref(),
            thread_id,
            home.path(),
            &project_path,
            Some(parent_thread_id),
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            scope.project_key,
            MemoryProjectKey::from_canonical_path(&project_path).unwrap()
        );
        let stored = runtime
            .memories()
            .memory_scope(thread_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.clanker_id, scope.clanker_id);
        assert_eq!(stored.project_key, scope.project_key);
        assert_eq!(stored.parent_thread_id, Some(parent_thread_id));
        assert_eq!(
            stored.recorded_at.timestamp(),
            scope.recorded_at.timestamp()
        );
    }

    #[tokio::test]
    async fn new_live_scope_uses_effective_turn_cwd_instead_of_stale_thread_metadata() {
        let home = tempdir().unwrap();
        let project = tempdir().unwrap();
        let effective_cwd = project.path().canonicalize().unwrap();
        let runtime = StateRuntime::init(home.path().to_path_buf(), "mock".to_string())
            .await
            .unwrap();
        let thread_id = ThreadId::new();
        seed_thread(&runtime, thread_id, "/stale/thread-start/project", None).await;

        let scope = current_thread_scope_with_identity(
            runtime.as_ref(),
            thread_id,
            home.path(),
            &effective_cwd,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            scope.project_key,
            MemoryProjectKey::from_canonical_path(&effective_cwd).unwrap()
        );
        assert_ne!(
            scope.project_key,
            MemoryProjectKey::from_canonical_path(Path::new("/stale/thread-start/project"))
                .unwrap()
        );
    }

    #[tokio::test]
    async fn historical_claim_uses_only_its_persisted_origin_and_stays_anonymous() {
        let home = tempdir().unwrap();
        let runtime = StateRuntime::init(home.path().to_path_buf(), "mock".to_string())
            .await
            .unwrap();
        let thread_id = ThreadId::new();
        let thread = seed_thread(
            &runtime,
            thread_id,
            "/historical/path",
            Some("https://user:secret@GitHub.com/example/history.git?token=no"),
        )
        .await;
        let scope = claimed_thread_scope(runtime.as_ref(), &thread)
            .await
            .unwrap();
        assert!(scope.clanker_id.is_none());
        assert_eq!(
            scope.project_key.as_str(),
            "v1:git:github.com/example/history"
        );
        assert_eq!(
            runtime.memories().memory_scope(thread_id).await.unwrap(),
            None
        );
    }

    async fn seed_thread(
        runtime: &StateRuntime,
        thread_id: ThreadId,
        cwd: &str,
        git_origin: Option<&str>,
    ) -> ThreadMetadata {
        let mut builder = ThreadMetadataBuilder::new(
            thread_id,
            format!("/tmp/{thread_id}.jsonl").into(),
            Utc::now(),
            SessionSource::Cli,
        );
        builder.cwd = cwd.into();
        builder.git_origin_url = git_origin.map(str::to_string);
        let thread = builder.build("mock");
        runtime.upsert_thread(&thread).await.unwrap();
        thread
    }

    fn write_character(home: &Path, id: &str, aliases: &[&str]) {
        let avatar = home.join("characters").join(id).join("avatar");
        std::fs::create_dir_all(&avatar).unwrap();
        std::fs::write(
            avatar.parent().unwrap().join("character.json"),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "id": id,
                "displayName": id,
                "aliases": aliases,
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
}
