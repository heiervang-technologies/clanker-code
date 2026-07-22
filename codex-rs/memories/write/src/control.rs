use std::path::Path;
use std::path::PathBuf;

use codex_state::MemorySelectionScope;

pub async fn clear_memory_roots_contents(codex_home: &Path) -> std::io::Result<()> {
    clear_authorized_roots([
        codex_home.join("memories"),
        codex_home.join("character_memories"),
        codex_home.join("memories_extensions"),
    ])
    .await
}

pub async fn clear_memory_selection_scopes(
    codex_home: &Path,
    scopes: &[MemorySelectionScope],
) -> std::io::Result<()> {
    let roots = scopes
        .iter()
        .map(|scope| match scope {
            MemorySelectionScope::Named {
                clanker_id,
                project_key,
            } => codex_home
                .join("character_memories")
                .join("characters")
                .join(clanker_id.as_str())
                .join("projects")
                .join(project_key.artifact_digest()),
            MemorySelectionScope::Anonymous => codex_home.join("memories"),
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut failures = Vec::new();
    for root in roots {
        let result = match ensure_no_symlink_components(codex_home, &root).await {
            Ok(()) => clear_memory_root_contents(&root).await,
            Err(err) => Err(err),
        };
        if let Err(err) = result {
            failures.push(format!("{}: {err}", root.display()));
        }
    }
    finish_clear_failures(failures)
}

async fn clear_authorized_roots(roots: impl IntoIterator<Item = PathBuf>) -> std::io::Result<()> {
    let mut failures = Vec::new();
    for root in roots {
        if let Err(err) = clear_memory_root_contents(root.as_path()).await {
            failures.push(format!("{}: {err}", root.display()));
        }
    }
    finish_clear_failures(failures)
}

fn finish_clear_failures(failures: Vec<String>) -> std::io::Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(std::io::Error::other(failures.join("; ")))
    }
}

async fn ensure_no_symlink_components(base: &Path, target: &Path) -> std::io::Result<()> {
    let relative = target.strip_prefix(base).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "memory root {} escapes {}",
                target.display(),
                base.display()
            ),
        )
    })?;
    let mut current = base.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("refusing symlinked memory path {}", current.display()),
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

pub(crate) async fn clear_memory_root_contents(memory_root: &Path) -> std::io::Result<()> {
    match tokio::fs::symlink_metadata(memory_root).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to clear symlinked memory root {}",
                    memory_root.display()
                ),
            ));
        }
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    tokio::fs::create_dir_all(memory_root).await?;

    let mut entries = tokio::fs::read_dir(memory_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_type = entry.file_type().await?;
        if file_type.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            tokio::fs::remove_file(path).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_character::CharacterCatalog;
    use codex_state::CanonicalClankerId;
    use codex_state::MemoryProjectKey;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn clear_memory_root_contents_preserves_root_directory() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("memories");
        let nested_dir = root.join("rollout_summaries");
        tokio::fs::create_dir_all(&nested_dir)
            .await
            .expect("create rollout summaries dir");
        tokio::fs::write(root.join("MEMORY.md"), "stale memory index\n")
            .await
            .expect("write memory index");
        tokio::fs::write(nested_dir.join("rollout.md"), "stale rollout\n")
            .await
            .expect("write rollout summary");

        clear_memory_root_contents(&root)
            .await
            .expect("clear memory root contents");

        assert!(
            tokio::fs::try_exists(&root)
                .await
                .expect("check memory root existence"),
            "memory root should still exist after clearing contents"
        );
        let mut entries = tokio::fs::read_dir(&root)
            .await
            .expect("read memory root after clear");
        assert!(
            entries
                .next_entry()
                .await
                .expect("read next entry")
                .is_none(),
            "memory root should be empty after clearing contents"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn clear_memory_root_contents_rejects_symlinked_root() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("outside");
        tokio::fs::create_dir_all(&target)
            .await
            .expect("create symlink target dir");
        let target_file = target.join("keep.txt");
        tokio::fs::write(&target_file, "keep\n")
            .await
            .expect("write target file");

        let root = dir.path().join("memories");
        std::os::unix::fs::symlink(&target, &root).expect("create memory root symlink");

        let err = clear_memory_root_contents(&root)
            .await
            .expect_err("symlinked memory root should be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            tokio::fs::try_exists(&target_file)
                .await
                .expect("check target file existence"),
            "rejecting a symlinked memory root should not delete the symlink target"
        );
    }

    #[tokio::test]
    async fn scoped_clears_preserve_other_memory_families() {
        let dir = tempdir().expect("tempdir");
        let chloe_a = named_selection(dir.path(), "chloe", project("project-a"));
        let chloe_b = named_selection(dir.path(), "chloe", project("project-b"));
        let anonymous_file = dir.path().join("memories/MEMORY.md");
        let chloe_a_root = selection_root(dir.path(), &chloe_a);
        let chloe_b_root = selection_root(dir.path(), &chloe_b);
        write_file(&anonymous_file, b"anonymous").await;
        write_file(&chloe_a_root.join("MEMORY.md"), b"chloe-a").await;
        write_file(&chloe_b_root.join("MEMORY.md"), b"chloe-b").await;

        clear_memory_selection_scopes(dir.path(), &[MemorySelectionScope::Anonymous])
            .await
            .unwrap();
        assert!(!tokio::fs::try_exists(&anonymous_file).await.unwrap());
        assert_eq!(
            tokio::fs::read(chloe_a_root.join("MEMORY.md"))
                .await
                .unwrap(),
            b"chloe-a"
        );
        assert_eq!(
            tokio::fs::read(chloe_b_root.join("MEMORY.md"))
                .await
                .unwrap(),
            b"chloe-b"
        );

        write_file(&anonymous_file, b"anonymous-new").await;
        clear_memory_selection_scopes(dir.path(), std::slice::from_ref(&chloe_a))
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read(&anonymous_file).await.unwrap(),
            b"anonymous-new"
        );
        assert!(
            !tokio::fs::try_exists(chloe_a_root.join("MEMORY.md"))
                .await
                .unwrap()
        );
        assert_eq!(
            tokio::fs::read(chloe_b_root.join("MEMORY.md"))
                .await
                .unwrap(),
            b"chloe-b"
        );
    }

    #[tokio::test]
    async fn all_local_clears_every_memory_family() {
        let dir = tempdir().expect("tempdir");
        for path in [
            dir.path().join("memories/MEMORY.md"),
            dir.path()
                .join("character_memories/characters/chloe/projects/a/MEMORY.md"),
            dir.path().join("memories_extensions/source/input.md"),
        ] {
            write_file(&path, b"data").await;
        }
        clear_memory_roots_contents(dir.path()).await.unwrap();
        for root in ["memories", "character_memories", "memories_extensions"] {
            let mut entries = tokio::fs::read_dir(dir.path().join(root)).await.unwrap();
            assert!(entries.next_entry().await.unwrap().is_none(), "{root}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn all_local_rejects_one_symlink_and_still_clears_other_roots() {
        let dir = tempdir().expect("tempdir");
        let outside = dir.path().join("outside");
        write_file(&outside.join("keep.txt"), b"keep").await;
        std::os::unix::fs::symlink(&outside, dir.path().join("memories")).unwrap();
        let named_file = dir
            .path()
            .join("character_memories/characters/chloe/projects/a/MEMORY.md");
        let extension_file = dir.path().join("memories_extensions/source/input.md");
        write_file(&named_file, b"named").await;
        write_file(&extension_file, b"extension").await;

        let err = clear_memory_roots_contents(dir.path())
            .await
            .expect_err("symlinked anonymous root must fail");

        assert!(
            err.to_string()
                .contains("refusing to clear symlinked memory root")
        );
        assert_eq!(
            tokio::fs::read(outside.join("keep.txt")).await.unwrap(),
            b"keep"
        );
        assert!(!tokio::fs::try_exists(named_file).await.unwrap());
        assert!(!tokio::fs::try_exists(extension_file).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scoped_clear_rejects_symlink_path_and_still_clears_other_scope() {
        let dir = tempdir().expect("tempdir");
        let chloe_a = named_selection(dir.path(), "chloe", project("project-a"));
        let chloe_b = named_selection(dir.path(), "chloe", project("project-b"));
        let chloe_a_root = selection_root(dir.path(), &chloe_a);
        let chloe_b_root = selection_root(dir.path(), &chloe_b);
        let outside = dir.path().join("outside");
        write_file(&outside.join("keep.txt"), b"keep").await;
        tokio::fs::create_dir_all(chloe_a_root.parent().unwrap())
            .await
            .unwrap();
        std::os::unix::fs::symlink(&outside, &chloe_a_root).unwrap();
        write_file(&chloe_b_root.join("MEMORY.md"), b"clear").await;

        let err = clear_memory_selection_scopes(dir.path(), &[chloe_a, chloe_b])
            .await
            .expect_err("symlinked scope must fail");
        assert!(err.to_string().contains("refusing symlinked memory path"));
        assert_eq!(
            tokio::fs::read(outside.join("keep.txt")).await.unwrap(),
            b"keep"
        );
        assert!(
            !tokio::fs::try_exists(chloe_b_root.join("MEMORY.md"))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn anonymous_git_workspace_cannot_observe_named_artifacts() {
        let dir = tempdir().expect("tempdir");
        let anonymous_root = dir.path().join("memories");
        crate::workspace::prepare_memory_workspace(&anonymous_root)
            .await
            .unwrap();
        let named = named_selection(dir.path(), "chloe", project("project-a"));
        write_file(
            &selection_root(dir.path(), &named).join("MEMORY.md"),
            b"named",
        )
        .await;
        let diff = crate::workspace::memory_workspace_diff(&anonymous_root)
            .await
            .unwrap();
        assert!(!diff.has_changes());
    }

    fn selection_root(home: &Path, scope: &MemorySelectionScope) -> PathBuf {
        match scope {
            MemorySelectionScope::Named {
                clanker_id,
                project_key,
            } => home
                .join("character_memories/characters")
                .join(clanker_id.as_str())
                .join("projects")
                .join(project_key.artifact_digest()),
            MemorySelectionScope::Anonymous => home.join("memories"),
        }
    }

    fn project(name: &str) -> MemoryProjectKey {
        MemoryProjectKey::from_git_origin(format!("git@github.com:example/{name}.git").as_str())
            .unwrap()
    }

    fn named_selection(
        home: &Path,
        id: &str,
        project_key: MemoryProjectKey,
    ) -> MemorySelectionScope {
        let package = home.join("characters").join(id);
        let avatar = package.join("avatar");
        fs::create_dir_all(&avatar).unwrap();
        fs::write(
            package.join("character.json"),
            format!(r#"{{"schemaVersion":1,"id":"{id}","displayName":"{id}","avatar":"avatar/avatar.json"}}"#),
        )
        .unwrap();
        fs::write(
            avatar.join("avatar.json"),
            r#"{"renderMode":"ansi-half-block","spritesheetPath":"sheet.ppm","frame":{"width":24,"height":24,"columns":1,"rows":1}}"#,
        )
        .unwrap();
        fs::write(
            avatar.join("sheet.ppm"),
            format!("P3\n24 24\n255\n{}", "0 0 0\n".repeat(24 * 24)),
        )
        .unwrap();
        MemorySelectionScope::Named {
            clanker_id: CanonicalClankerId::resolve_exact(&CharacterCatalog::load(home), id)
                .unwrap(),
            project_key,
        }
    }

    async fn write_file(path: &Path, body: &[u8]) {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, body).await.unwrap();
    }
}
