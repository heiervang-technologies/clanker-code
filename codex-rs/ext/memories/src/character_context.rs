// Modified by Heiervang Technologies.
use codex_character::CharacterCatalog;
use codex_context_fragments::CharacterMemoryContext;
use codex_context_fragments::ContextualUserFragment;
use codex_state::MemorySelectionScope;
use codex_state::ScopedMemoryRecord;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_output_truncation::approx_token_count;
use std::fmt::Write as _;
use std::path::Path;

pub(crate) const CHARACTER_MEMORY_CONTEXT_MAX_TOKENS: usize = 1_500;
const CHARACTER_MEMORY_BODY_MAX_TOKENS: usize = 1_000;
pub(crate) const CHARACTER_MEMORY_SELECTION_LIMIT: usize = 128;
const TRUNCATION_MARKER: &str = "\n[truncated]";

pub(crate) async fn build_character_memory_context(
    codex_home: &AbsolutePathBuf,
    scope: &MemorySelectionScope,
    records: &[ScopedMemoryRecord],
) -> Option<String> {
    let MemorySelectionScope::Named {
        clanker_id,
        project_key,
    } = scope
    else {
        return None;
    };
    let display_name = CharacterCatalog::load(codex_home)
        .resolve(clanker_id.as_str())
        .ok()
        .filter(|resolved| resolved.manifest.id == clanker_id.as_str())
        .map_or_else(
            || clanker_id.as_str().to_string(),
            |resolved| resolved.manifest.display_name,
        );
    let mut body = format!(
        "schema_version=1\ncharacter_id={}\ndisplay_name={}\nproject_digest={}\n\
citation_guidance=If and only if a supplied record is materially relied upon, append exactly one \
final-only citation block after the answer. Use this exact parser format, cite only used records, \
put one supplied citation_path:citation_lines|note=[short use] entry per line, and put each supplied \
source_thread_id UUID once in rollout_ids:\n\
<oai-mem-citation>\n<citation_entries>\nrollout_summaries/example.md:10-12|note=[how memory was used]\n\
</citation_entries>\n<rollout_ids>\n00000000-0000-0000-0000-000000000001\n\
</rollout_ids>\n</oai-mem-citation>\n",
        xml_escape(&json_string(clanker_id.as_str())),
        xml_escape(&json_string(&display_name)),
        project_key.artifact_digest(),
    );
    if rendered_tokens(&body) > CHARACTER_MEMORY_CONTEXT_MAX_TOKENS {
        return None;
    }

    let memory_root = codex_memories_write::memory_root_for_scope(codex_home, scope);
    let mut added = 0usize;
    for record in records {
        let Some(source) = validated_artifact_body(&memory_root, record).await else {
            continue;
        };
        let Some(candidate) = pack_record(&body, record, source) else {
            continue;
        };
        body = candidate;
        added += 1;
    }

    (added > 0).then(|| CharacterMemoryContext::new(body).render())
}

async fn validated_artifact_body<'a>(
    memory_root: &Path,
    record: &'a ScopedMemoryRecord,
) -> Option<&'a str> {
    let citation = record.citation_path.as_ref()?;
    let expected_citation = format!("rollout_summaries/{}.md", record.output.thread_id);
    if citation.as_str() != expected_citation {
        return None;
    }
    let path = memory_root.join(citation.as_str());
    let metadata = tokio::fs::symlink_metadata(&path).await.ok()?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return None;
    }
    let expected = codex_memories_write::render_scoped_rollout_summary(record).ok()?;
    if metadata.len() != u64::try_from(expected.len()).ok()? {
        return None;
    }
    let actual = tokio::fs::read_to_string(path).await.ok()?;
    if actual != expected {
        return None;
    }
    codex_memories_write::scoped_episode_body(record)
}

fn pack_record(current_body: &str, record: &ScopedMemoryRecord, source: &str) -> Option<String> {
    let escaped = escaped_episode_body(source, false);
    let complete = render_record(current_body, record, source, &escaped)?;
    if record_fits(&escaped, &complete) {
        return Some(complete);
    }

    let boundaries = source
        .char_indices()
        .map(|(index, _)| index)
        .skip(1)
        .collect::<Vec<_>>();
    let mut low = 1usize;
    let mut high = boundaries.len();
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let end = boundaries[middle - 1];
        let represented = &source[..end];
        let escaped = escaped_episode_body(represented, true);
        let candidate = render_record(current_body, record, represented, &escaped)?;
        if record_fits(&escaped, &candidate) {
            best = Some(candidate);
            low = middle + 1;
        } else {
            high = middle.saturating_sub(1);
        }
    }
    best
}

fn record_fits(escaped_body: &str, candidate: &str) -> bool {
    approx_token_count(escaped_body) <= CHARACTER_MEMORY_BODY_MAX_TOKENS
        && rendered_tokens(candidate) <= CHARACTER_MEMORY_CONTEXT_MAX_TOKENS
}

fn render_record(
    current_body: &str,
    record: &ScopedMemoryRecord,
    represented: &str,
    escaped_body: &str,
) -> Option<String> {
    if represented.is_empty() {
        return None;
    }
    let source_character = record.clanker_id.as_ref()?;
    let source_project = record.project_key.as_ref()?;
    let citation = record.citation_path.as_ref()?;
    let line_start = 11 + usize::from(record.output.git_branch.is_some());
    let line_end = line_start + represented.lines().count().saturating_sub(1);
    let mut candidate = current_body.to_string();
    candidate.push_str("<memory_record>\n");
    writeln!(candidate, "source_thread_id={}", record.output.thread_id).ok()?;
    writeln!(
        candidate,
        "source_character_id={}",
        source_character.as_str()
    )
    .ok()?;
    writeln!(
        candidate,
        "source_project_digest={}",
        source_project.artifact_digest()
    )
    .ok()?;
    writeln!(candidate, "visibility={}", record.visibility.as_str()).ok()?;
    if let Some(parent_thread_id) = record.parent_thread_id {
        writeln!(candidate, "parent_thread_id={parent_thread_id}").ok()?;
    }
    writeln!(candidate, "citation_path={}", citation.as_str()).ok()?;
    writeln!(candidate, "citation_lines={line_start}-{line_end}").ok()?;
    candidate.push_str("<episode_summary>\n");
    candidate.push_str(escaped_body);
    candidate.push_str("\n</episode_summary>\n</memory_record>\n");
    Some(candidate)
}

fn escaped_episode_body(represented: &str, truncated: bool) -> String {
    let mut escaped = xml_escape(represented);
    if truncated {
        escaped.push_str(TRUNCATION_MARKER);
    }
    escaped
}

fn rendered_tokens(body: &str) -> usize {
    approx_token_count(&CharacterMemoryContext::new(body).render())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
#[path = "character_context_tests.rs"]
mod tests;
