# Fork Drift Patterns

Catalog of recurring drift shapes seen while maintaining HT forks via periodic rebase syncs. This document is consumed by the `fork-sync-conflict` skill (under `skills/fork-sync-conflict/SKILL.md`) to drive autonomous conflict resolution.

This is the **public/internal** version — operational secrets, sync history, and agent identities live in the local `FORK_MAINTENANCE_DIARY.md` only.

## Drift Categories

### 1. File Restructuring Drift

**What**: Upstream reorganizes files or directories that HT has modified.

**Examples**:
- `ht-unsloth studio/setup.sh`: upstream repeatedly restructures venv setup logic. **Standing rule: always take upstream** (`git checkout --theirs studio/setup.sh`).
- `ht-ACE-Step api_server.py`: upstream modularized a monolithic file into `acestep/api/` submodules; HT had int8 quantization changes in the same file.

**Resolution**: take upstream's structure, port HT's change into the new layout. For known files with standing rules, follow them.

**Frequency**: high for actively developed upstreams.

---

### 2. Registry / Config Expansion Drift

**What**: Upstream adds new entries to registries, model configs, or lookup tables in the same region where HT added entries.

**Examples**:
- `ht-vllm registry.py`: HT added Qwen2.5-Omni model entries; upstream later added ColQwen3 entries in the same registry block.
- `ht-vllm-omni pyproject.toml`: dependency version bumps and new entries near HT's additions.

**Resolution**: keep both sets of entries. Check if upstream now includes any of HT's additions (drop duplicates). Verify no conflicting version constraints.

**Frequency**: medium. Registries grow monotonically so conflicts recur as long as HT carries its own entries.

---

### 3. API Surface / Signature Drift

**What**: Upstream changes function signatures, class interfaces, or API contracts that HT code depends on or extends.

**Examples**:
- `ht-vllm-omni serving_speech.py / api_server.py`: upstream refactored serving endpoints; HT's speaker embedding and streaming features needed porting to new signatures.
- `ht-llama.cpp server-common.h / server-context.cpp`: upstream restructured server internals; HT's remap-developer-role feature needed adapting.

**Resolution**: understand the new API shape, then port HT's feature logic into it. Most labor-intensive drift type — often requires reading upstream's PR to understand intent.

**Frequency**: medium-low per repo, but high impact when it happens.

---

### 4. CI / Build System Drift

**What**: Upstream changes CI workflows, build configs, or tooling that HT has also modified.

**Examples**:
- `ht-llama.cpp` GitHub Actions workflows: upstream reorganized them; HT had modifications in the same files.
- `ht-llama.cpp CONTRIBUTING.md`: documentation conflicts from parallel edits.

**Resolution**: usually take upstream's version for CI unless HT has specific build requirements. Verify HT-specific CI steps (if any) are preserved. The `fork-sync.yml` and `conflict-resolver.yml` workflows themselves are HT-owned and should always be `--ours`.

**Frequency**: low-medium. Spiky around upstream release cycles.

---

### 5. UI / Frontend Drift

**What**: Upstream updates frontend components that HT has customized.

**Examples**:
- `ht-llama.cpp` Svelte UI components: HT added a cancel-button feature; upstream restructured the chat UI components (10+ file conflicts in one sync).

**Resolution**: identify HT's feature additions (new components, new props) and graft them onto upstream's updated component tree. Test that features still render correctly.

**Frequency**: low for most forks. High for repos with active frontend development.

---

### 6. Upstream Convergence Drift

**What**: Upstream independently implements functionality that overlaps with HT's changes — not because HT upstreamed it, but because both sides needed the same thing. Alternatively, contributions go upstream via a personal fork (`marksverdhei`), and when merged, the HT fork's original commits conflict with the (possibly modified) upstream version.

**Key distinction**: HT org forks are not for upstreaming. Features meant for upstream go to the personal fork (`marksverdhei`), from which branches can be cherry-picked or copied. The `ht` branch is intended to carry HT-specific changes indefinitely, leaving upstream in peace.

**Examples**:
- `ht-vllm-omni` speaker embedding PR (upstream #1227): contributed via personal fork, accepted upstream with co-author modifications. Upstream version is a superset. Two HT commits (embedding passthrough + voices API) needed manual skipping during rebase since git couldn't detect they implemented the same feature.

**Resolution**: when upstream gains functionality that overlaps with HT commits, identify which HT commits are now redundant and `git rebase --skip` them when they become empty. The upstream version should be preferred.

**Frequency**: medium. Occurs when personal fork contributions are accepted, or when upstream naturally converges on the same features.

---

### 7. Duplicate Implementation Drift

**What**: Both HT and upstream independently implement the same feature with different function names, validation logic, or code structure. After rebase, both implementations coexist silently.

**Examples**:
- `ht-vllm-omni` streaming audio: HT implemented `_stream_progressive_audio` and `_make_wav_header`; upstream independently added `_generate_audio_chunks` and `_create_wav_header`. Both exist after rebase, doubling the surface area. **Concrete cost** (2026-05-09 prod incident): caused engine death after ~1-2 requests in production; required a 3+ hour incident response and a rollback to `speaker-alias-v1`. Tracked in [ht-vllm-omni#39](https://github.com/heiervang-technologies/ht-vllm-omni/issues/39) as a must-fix-in-rebase. Code2Wav file was byte-identical between the working era (`d6fb9083`) and current `ht` HEAD, confirming the latent bug was introduced by HT's `d5692616` (progressive WAV streaming, 2026-04-22) and survived undetected because no prior production load reached the trigger threshold.
- `ht-vllm-omni` `stream` field: HT added `stream: bool` to protocol; upstream later added the same field with richer pydantic validation. If both survive rebase, pydantic silently uses the last definition.

**Resolution**: after every sync, diff `ht` against `upstream/main` and audit each modified file for duplicate helpers, redundant validation, and dead code paths. When upstream adds equivalent functionality, remove HT's version and adopt upstream's pattern. If HT's version is genuinely better, contribute it via the personal fork — but the HT org fork should not carry both implementations long-term.

**Process note from the 2026-05-09 incident**: when a Cat 7 duplicate causes a prod incident, file an issue capturing diagnostic context (suspected commit, byte-level evidence, why prior production didn't surface it) BEFORE the rollback erases the evidence trail. Otherwise the eventual rebase will just re-introduce the bug.

**Frequency**: medium-high for forks where HT and upstream are working on the same features concurrently.

---

### 8. Metadata / Attribution Drift

**What**: Small metadata changes (copyright headers, author fields, license notices) accumulate in the fork and create unnecessary diff noise or legal ambiguity.

**Examples**:
- `ht-vllm-omni cuda_graph_decoder_wrapper.py`: copyright header changed from "The Alibaba Qwen team" to "Heiervang Technologies" on a file HT only slightly modified. Legally questionable and creates conflict noise.

**Resolution**: never modify copyright/attribution on upstream files unless HT is the sole author. If contributing via personal fork, strip such changes first. During rebase, take upstream's metadata.

**Frequency**: low but persistent. Often introduced accidentally during development.

---

### 9. History Rewrite Drift

**What**: Upstream rebases or force-pushes their main branch, causing the HT mirror to diverge.

**Examples**:
- `ht-codex`: upstream rebased history — 42 commits on HT's main not in upstream, 412 new upstream commits. Required `git reset --hard upstream/main && git push --force` on `main` (not `ht`).

**Resolution**: force-reset `main` to `upstream/main`, then rebase `ht` onto new `main`. **Operator territory, not autopilot.** Force-pushing `main` violates the autopilot's hard safety rule #1 (no push to `main`/`master`), and verifying that the backup tag is fresh enough to roll back to requires human judgment about how much fork-side work post-dates it. The autopilot must bail on Cat 9 and let the issue-fallback file a tracking ticket.

**Frequency**: rare. Only seen with newer / less-stable upstreams.

---

### 10. Deployment Governance Drift / Unsanctioned Image Channel

**What**: An image reaches prod via a path that bypasses the fork's CI pipeline. An operator manually `skopeo`-copies an upstream image into the fork's registry namespace, sometimes hand-tagging it with branch suffixes (`-main`, `-ht`, `-voices`) that the fork's own `docker-publish.yml` does not produce. The image then looks plausibly home-grown but lacks any HT-specific patches.

**Examples**:
- `ht-vllm-omni 1cd52104-main` (2026-05-09 incident): operator pinned this tag in `cloud/k8s/ai/vllm-omni-tts.yaml` while `ht` had a known compat issue with current vllm. The YAML comment documented the reason. After `ht` caught up (commit `739c1e66` bumped the vllm base), the comment became stale but the pin was not updated. Crashed every `/v1/audio/speech` because upstream `1cd52104` predated HT's speaker/voice alias fix (cherry-pick of upstream PR#2424 that `ht` carried as `7ae20e10`). Producer was manual `skopeo`, not any workflow.

**Resolution pattern**:
- **Tag-suffix gate at the registry side**: deployment-time admission rule that refuses any image whose tag does not end in the fork-owned suffix (`-ht` for HT forks). Belongs in `cloud/`, not the fork.
- When operators reach for upstream-pinned images, treat it as a signal that the fork has fallen behind — resolve the parked rebase rather than let the temporary pin become permanent. If a YAML comment claims the fork is incompatible, verify against current fork `HEAD` before trusting it.
- Fork's `docker-publish.yml` should set explicit `type=ref,event=branch` + `type=sha,suffix={{branch}}` so its own production tags carry a branch suffix and can be recognized by the registry-side rule.
- This category is also relevant for [Phase 5 security scanning](FORK_AUTOMATION_PLAN.md#phase-5-upstream-security-scan-future--multi-step-extension) — manually-skopeo'd images bypass any future security-scan job too. The registry gate is the chokepoint.

**Frequency**: rare in absolute terms (operator decision under pressure), but the impact is full-prod-down for the affected service and the diagnostic chain is non-obvious from fork-side alone — requires correlating registry tags with deployment manifest with operator history. High debugging cost.

**Detection**: audit each fork's deployment manifest against the set of tags producible by that fork's own `docker-publish.yml`. Any tag in prod not in the producible set is a candidate.

---

### 11. Silent Merge Anomalies (Displacement + Survival Duals)

**What**: Two failure modes that share a detection surface but invert their failure direction. Both are invisible to `git status` and to a plain `git diff` review — the merge completes "cleanly", no conflict marker, and the resulting tree looks plausible.

**11a — Silent Displacement (HT content discarded by upstream)**:
A both-touched file auto-merges with upstream's version winning entirely. HT's delta vanishes. Failure shape: HT improvement / correctness change silently dropped.

**11b — Silent Survival (dropped-commit content persists)**:
A merge commit message documents "Drops `<SHA>`: `<subject>`", but `<SHA>` modified a file that has no upstream side. The merge can discard the COMMIT lineage but cannot revert the file CONTENT — there is no upstream alternative to triangulate against. Failure shape: a commit listed as dropped still ships its effect.

**Detection — three-way delta triangulation** (covers both):

```bash
MB=$(git merge-base upstream/main origin/ht)
# Rename-aware both-touched list (path-equality misses upstream renames):
ht_files=$(git log --diff-filter=AMR --name-only $MB..origin/ht | sort -u)
up_files=$(git log --diff-filter=AMR --name-only $MB..upstream/main | sort -u)
both=$(comm -12 <(echo "$ht_files") <(echo "$up_files"))

for f in $both; do
  ht=$(git diff $MB..origin/ht -- "$f" | wc -l)
  up=$(git diff $MB..upstream/main -- "$f" | wc -l)
  mvU=$(git diff upstream/main -- "$f" | wc -l)   # after merge, vs upstream
  printf '%-70s ht=%-5d up=%-5d mvU=%-5d\n' "$f" $ht $up $mvU
done
```

**Signature**: any line with `ht` large AND `mvU` ≈ 0 is silent displacement — the merged file is identical to upstream, HT delta gone. The triangulation cannot distinguish intentional (Cat 6 convergence) from accidental — that classification is manual.

**Survival check** (separate pass, for each "Drops `<SHA>`" line in the merge message):

```bash
DROPPED=<sha>
for f in $(git show --name-only --pretty=format: $DROPPED); do
  if git ls-tree upstream/main -- "$f" | grep -q .; then
    echo "$f: upstream-touched — covered by displacement triangulation"
  else
    echo "$f: HT-only — VERIFY MANUALLY that $DROPPED's content effect is undone"
  fi
done
```

For each HT-only file, read the dropped commit's diff and confirm each `+`/`-` line is matched (or supplanted) in the merged tree. If not, follow-up commit needed.

**Examples (both from the 2026-05-12 ht-vllm-omni rebase, found in one pass)**:
- **Displacement (benign Cat 6)**: `examples/online_serving/qwen3_tts/openai_speech_client.py`. HT delta = 13 (`payload["voice"] = args.speaker` rename); upstream delta = 268 (file renamed + rewritten to `.../text_to_speech/qwen3_tts/openai_speech_client.py`). Merged tree = upstream version; the rename carries upstream's logic which already implements HT's intent. Plain path-equality misses this — the rename-aware list is required.
- **Survival (silent failure)**: `docker/Dockerfile.slim`. HT commit `739c1e66` ("build: bump vllm base to v0.19.1") was documented as dropped in the merge message. But Dockerfile.slim is HT-only (introduced by `406f2448`), so the merge couldn't revert `v0.19.1`. File content survived at v0.19.1 despite the lineage drop. Caught by snoop-kube during image build setup. Follow-up commit `d1ad5ee1` ("build: default Dockerfile.slim base to vllm-openai v0.20.0") fixed it.

**Resolution**: bake the triangulation and survival checks into the post-sync hygiene step (see checklist below). Cost: ~5 minutes per rebase. Catches a class of regressions that diff-review alone does not.

**Frequency**: per-rebase risk on any fork with non-trivial both-touched files OR explicit drop-lines in merge messages. Higher for forks where upstream is restructuring directories (Cat 1 amplifies displacement risk).

---

## Per-Fork Drift Profile

| Fork | Primary Drift Types | Chronic Conflicts | Notes |
|------|--------------------|--------------------|-------|
| ht-ACE-Step-1.5 | File restructuring | `api_server.py` (resolved, now modular) | Upstream actively restructuring |
| ht-codex | History rewrite | None currently | Young upstream, expect instability |
| ht-llama.cpp | API surface, CI, UI, silent merge anomalies | Server internals during major releases | Very active upstream, high churn. Rerere near-miss 2026-05-12 on `server-models.cpp` — see SKILL.md "rerere caveat". |
| ht-LlamaFactory | Minimal | None observed | Low HT diff |
| ht-mergekit | Minimal | None observed | Low HT diff. Active 2026-05-13. |
| ht-pytorch | Minimal | None | Pure additive HT delta (docs + CI) |
| ht-unsloth | File restructuring | `studio/setup.sh` (every sync) | **Standing rule: take upstream** |
| ht-vibe | None observed | None | Upstream less active |
| ht-vllm | Registry expansion | Model registry blocks | HT entries are permanent fork additions |
| ht-vllm-omni | API surface, registry, convergence, duplicate impl, silent merge anomalies | Serving endpoints, streaming audio | Most complex HT diff. Upstream convergence via personal fork contributions. Both 11a (displacement) and 11b (survival) duals caught in 2026-05-12 rebase. |
| ht-voxcii | None observed | None | Low HT diff |

## Post-Sync Hygiene Checklist

After each sync, especially for high-drift forks (vllm-omni, llama.cpp):

1. **Diff audit**: `git diff origin/main..origin/ht` — review every file for duplicate validation, redundant helpers, dead code.
2. **Convergence check**: if upstream gained functionality overlapping with HT commits, verify redundant HT commits were dropped during rebase.
3. **Copyright scan**: verify no upstream copyright headers were accidentally modified.
4. **Protocol field check**: for forks with shared protocol definitions, verify no duplicate field definitions survived rebase.
5. **Prefer upstream patterns**: when upstream added the same feature differently, adopt their approach and remove HT's version.
6. **Silent merge anomaly check (Cat 11)**: run the three-way delta triangulation against the rename-aware both-touched set and flag any `mvU≈0` row for manual displacement classification. For every "Drops `<SHA>`" line in the merge message, run the survival check on HT-only files touched by `<SHA>`.
7. **Native-build gate (forks with non-script targets)**: for forks that ship native binaries (e.g. ht-llama.cpp's C++ server, Tauri shells), the drift-zero check must include a fresh build (`cmake --build`, `cargo build`, `npm run build`), not just a typecheck of the script layer. A `tsc`-clean tree can still ship a backend that fails to compile — caught on ht-llama.cpp 2026-05-12 when a missing close-brace in `tools/server/server-models.cpp` produced a 200+-error cascade only visible under `cmake`.

## See also

- `FORK_CHECKLIST.md` — fork setup checklist (one-time, per new fork).
- `FORK_MANAGEMENT.md` — broader fork management guide.
- `skills/fork-sync-conflict/SKILL.md` — the autonomous resolution recipe that consumes this document.
