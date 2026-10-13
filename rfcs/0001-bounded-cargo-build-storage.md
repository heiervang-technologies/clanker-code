# RFC 0001: Bounded Cargo build storage

- Status: Proposed
- Created: 2026-08-06
- Owners: Clanker build and developer tooling
- Supersedes: none
- Related: PR #31, `scripts/cargo_target_size.py`

## Summary

Clanker should treat local Cargo build output as a bounded, observable resource.
The recommended architecture uses Cargo's stable `build.build-dir` setting to
keep final artifacts in each worktree's normal `target/` while moving
intermediate output under
`{cargo-cache-home}/clanker/build/{workspace-path-hash}`. This creates one
Clanker-owned cache root without forcing concurrent worktrees to share a Cargo
lock domain.

Managed build commands should report both the current worktree and aggregate
linked-worktree usage, warn before the cache becomes disruptive, and refuse to
start another high-growth build after a hard limit is crossed. Cleanup should be
explicit, safe, and available for either the current worktree or selected linked
worktrees.

The proposed default budgets are:

| Scope | Warning | Hard limit |
|---|---:|---:|
| One worktree build directory | 40 GiB | 80 GiB |
| Clanker intermediate-cache root | 100 GiB | 160 GiB |

Crossing a warning limit prints a concise diagnosis and cleanup command.
Crossing a hard limit prevents managed commands such as `just test`, `just fix`,
and local release builds from starting until storage is reclaimed or the user
sets an explicit, one-command override. The guard does not delete files
automatically.

## Motivation

On 2026-08-06, `du` attributed 333 GiB of filesystem blocks to the primary
Clanker checkout's `codex-rs/target`. `cargo clean` removed 310,061 files and
reported 355.7 GiB of logical file content. A linked-worktree scan attributed
another 256 GiB of blocks to 11 target directories, including individual
directories of 86 GiB and 92 GiB.

Cleaning all linked target directories removed 659,784 generated file entries.
Before deletion, `du` attributed approximately 590 GiB of blocks to those paths;
Cargo reported 627.9 GiB of logical file content. The Btrfs filesystem's free
space did not materially increase after deletion and a filesystem sync, and no
large deleted build artifacts remained open. The extents were therefore still
shared or retained by snapshots. This is important: neither `du` block counts
nor logical file sizes equal exclusive physical bytes or bytes that deletion
will immediately reclaim.

The generated namespace was still unacceptably large, but the incident also
shows that the current measurement semantics are not clear enough. Reports must
distinguish logical content, blocks attributed to paths, exclusive physical
storage when the filesystem can provide it, and observed free-space changes.

PR #31 already reduced routine artifact size by disabling incremental
compilation and debug information in the default development and test profiles.
It also added `just target-size`. Those were useful mitigations, but they do not
bound growth:

- old artifacts survive profile, toolchain, feature, and dependency changes;
- each worktree gets an independent target directory;
- the existing command only inspects one target directory;
- it reports logical file sizes but not block attribution, exclusive storage,
  or observed reclaim;
- no command warns, stops, or offers a safe pruning workflow.

The result is silent, effectively unbounded growth until the filesystem is under
pressure.

## Research findings

- Cargo 1.91 stabilized separate `target-dir` and `build-dir` locations. Final
  artifacts stay in `target-dir`; compiler, dependency, fingerprint, and build
  script intermediates live in `build-dir`. Clanker pins Cargo 1.97. See the
  [Cargo build-cache documentation](https://doc.rust-lang.org/cargo/reference/build-cache.html)
  and [Rust 1.91 release notes](https://doc.rust-lang.org/stable/releases.html#version-1910-2025-10-30).
- `build.build-dir` supports `{cargo-cache-home}` and
  `{workspace-path-hash}`. Cargo upstream is considering the same general
  layout as a future default, while warning that projects must test tools that
  depend on Cargo's internal build layout. See the
  [configuration reference](https://doc.rust-lang.org/cargo/reference/config.html#buildbuild-dir)
  and [cargo#16147](https://github.com/rust-lang/cargo/issues/16147).
- Cargo's stable automatic garbage collection covers global downloaded and
  extracted caches, not a size budget for workspace build directories. See the
  [Cargo 1.88 changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-188-2025-06-26).
- Cargo 1.97 refuses `cargo clean --target-dir` when the path does not look like
  a Cargo target directory. Clanker's cleanup tooling should preserve that
  defense. See the
  [Cargo 1.97 changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-197-2026-07-09).
- `sccache` can share compiler results between worktrees and caps its local
  cache with `SCCACHE_CACHE_SIZE`, but it does not cache every Rust output. It
  is a rebuild accelerator, not the storage policy. See the
  [sccache Rust documentation](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)
  and [local-cache configuration](https://github.com/mozilla/sccache/blob/main/docs/Configuration.md#disk-local).

## Goals

- Keep routine local Clanker build output within an explicit disk budget.
- Make total usage across linked worktrees visible from any worktree.
- Make reclaiming generated output safe and obvious.
- Preserve an escape hatch for deliberate profiling and full-matrix work.
- Keep the implementation portable across Linux, macOS, and Windows.
- Avoid surprising deletion during an ordinary build or test command.

## Non-goals

- Managing Cargo's global registry and Git caches.
- Managing Bazel, npm, model, application, or operating-system caches.
- Guaranteeing that direct `cargo` invocations obey repository policy.
- Making every worktree share one target directory.
- Preserving target artifacts as durable build outputs.

## Proposal

### 1. Separate final artifacts from intermediate output

Add this checked-in Cargo configuration:

```toml
[build]
build-dir = "{cargo-cache-home}/clanker/build/{workspace-path-hash}"
```

Keep `target-dir` at its default. Final binaries, documentation, and package
output remain discoverable under each worktree's `target/`. Large dependency,
fingerprint, build-script, and compiler intermediates move into one managed
Clanker cache root.

The workspace-path hash preserves one Cargo lock domain per worktree. A fixed,
shared build directory is not proposed because concurrent agents would contend
on the same Cargo lock and mix branch/feature invalidation state.

Before rollout, audit the repository for assumptions about `target/debug/deps`,
build-script `OUT_DIR`, and Cargo's internal layout. Validate the new layout on
Linux, macOS, and Windows.

### 2. Replace the single-directory report with a storage inventory

Extend the target-size tool to discover linked worktrees using
`git worktree list --porcelain` and query `cargo metadata` for both the target
and build directories. For each resolved directory, report:

- resolved path;
- unique logical bytes, counting hard-linked files once per target;
- filesystem blocks attributed to the path when the platform exposes them;
- exclusive physical bytes when the filesystem exposes that information
  without elevated privileges;
- last artifact modification time;
- top-level profile breakdown such as `debug`, `release`, and `profiling`.

The default output should end with per-worktree and aggregate totals. A JSON mode
should expose the same data for tests and future automation. The report must
label every measurement. It must not call logical or path-attributed bytes
"physically reclaimable" without evidence. Cleanup commands should also report
filesystem free space before and after deletion, while noting that snapshots,
compression, reflinks, and delayed allocation can affect the result.

Budget enforcement uses unique logical bytes. That metric is conservative and
portable, and it bounds the generated artifact set independently of filesystem
compression, reflinks, and snapshot policy. Physical and path-attributed
measurements remain diagnostics rather than enforcement inputs.

The tool should honor `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR` for the
current worktree. Worktrees with explicit directories outside their checkout
may be added through repeatable command-line options. Paths are resolved before
inspection, and duplicate directories are counted only once.

### 3. Add explicit cleanup commands

Provide these repository commands:

- `just build-cache-clean-current`: run Cargo's supported cleanup for the
  current worktree.
- `just build-cache-clean-worktrees`: print linked build directories, their
  sizes, and the exact directories proposed for cleanup; require explicit
  confirmation before cleaning non-current worktrees.
- `just build-cache-clean-worktrees --dry-run`: report what would be removed
  without mutating anything.

For a live worktree, cleanup must invoke `cargo clean` with that worktree's
manifest and configuration so Cargo resolves both its target and build
directories. It must reject symlinked directories and paths outside the resolved
Clanker cache root. If Cargo rejects a missing or invalid `CACHEDIR.TAG`, the
tool must stop and explain the issue; it must not silently bypass Cargo's safety
check. An orphaned hash directory may be removed only after the tool confirms
that its owning worktree no longer exists, resolves it as a direct child of the
cache root, and validates its Cargo cache marker.

No age-based or background deletion is proposed. Modification time is useful
for choosing what to clean, but it is not reliable evidence that another agent
or process has finished with a worktree.

The tool may automatically clean cache entries whose owning worktree no longer
exists. Cleaning a linked worktree always requires explicit selection.

### 4. Enforce budgets in managed high-growth commands

Add a fast preflight to repository-managed commands that can materially grow
the build directory, initially:

- `just test`;
- `just fix` and `just clippy`;
- local debug and release build recipes;
- packaging recipes that write into the normal Cargo build directory.

At the warning limit, the command proceeds after printing the largest build
directories and the relevant cleanup command. At the hard limit, it exits before
starting Cargo. Read-only reports and cleanup commands always remain available.

Defaults are 40/80 GiB for the current build directory and 100/160 GiB
aggregate. These values leave room for a scoped test build and several active
worktrees while
preventing a repeat of the incident. They should be reviewed after 30 days of
telemetry from normal scoped builds.

An exceptional build may set `CLANKER_BUILD_BUDGET_OVERRIDE_GIB` for that one
invocation. The preflight prints that an override is active and still reports
usage. The override is intentionally not a persistent config file: repeatedly
needing it means the defaults or build behavior should be revisited.

Direct `cargo` commands cannot be intercepted reliably and remain outside this
guard. Contributor and agent instructions should continue to prefer the `just`
recipes.

A literal host-level bound requires placing the central cache root on a
filesystem or subvolume with a quota. That is a useful optional workstation
policy, but it cannot be imposed portably by this repository.

### 5. Keep profiles compact and make exceptional artifacts explicit

Retain the compact `dev` and `test` profile settings introduced in PR #31.
Symbol-bearing builds should continue to use the explicit `profiling` profile.
Routine instructions should not recommend `--all-features`; scoped crate tests
remain the default.

The profiling profile counts toward reported usage but may use the one-command
budget override. This makes the cost visible without weakening the normal
policy.

### 6. Test the policy without producing a large cache

The inventory and policy logic should operate on injected repository, target,
and build paths so tests can use temporary fixture trees. Tests should cover:

- one worktree below, at, and above each threshold;
- aggregate usage across multiple linked worktrees;
- duplicate and symlinked target paths;
- `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR` handling;
- dry-run cleanup output;
- malformed or missing Cargo cache markers;
- logical-size behavior when filesystem block or exclusive-byte measurements
  are unavailable.

CI should test the tooling and threshold behavior with fixtures. It should not
grow an actual target directory merely to exercise the limit.

## Why not share one target directory?

A shared target can deduplicate dependency artifacts across worktrees, but it
also makes concurrent agents contend on Cargo locks and mixes feature, profile,
compiler, and branch churn into one invalidation domain. It changes build
latency and failure isolation in ways that need measurement. It is therefore not
the default proposed here.

An opt-in experiment may compare a shared target against isolated, bounded
targets. Any adoption would require evidence that saved storage outweighs lock
contention for concurrent agent workloads.

## Why not add sccache first?

`sccache` can make rebuilding after cleanup cheaper, but it creates a second
cache that also needs a size limit. It does not remove linked binaries, test
harnesses, build-script outputs, or stale fingerprints from Cargo target and
build directories. It is complementary, not a substitute for a storage budget.

After the basic guard lands, an opt-in `sccache` experiment should use an
explicit local cap and measure clean-build time, warm-build time, and total disk
usage before becoming a default.

## Alternatives considered

### Continue relying on compact Cargo profiles

Rejected. The incident happened after the repository had a size-report command
and compact profile defaults. These settings reduce the slope but do not impose
a bound or handle stale and duplicated worktree output.

### Automatically delete least-recently-used worktrees during builds

Rejected. File timestamps do not prove that a worktree is inactive, and an
ordinary build should not invalidate another agent's artifacts. Explicit cleanup
is slower but predictable and auditable.

### Run periodic unconditional `cargo clean`

Rejected as the primary mechanism. It hides the growth pattern, causes
surprising rebuilds, and is difficult to schedule portably. A user may still
schedule the explicit cleanup command locally.

### Use `cargo clean -p` for selective pruning

Insufficient. Much of the growth comes from many versions and configurations of
workspace-wide dependencies, test harnesses, and build scripts. Package cleanup
does not provide a reliable global budget.

## Rollout

1. Extend `scripts/cargo_target_size.py` with explicit measurement semantics,
   worktree aggregation, JSON output, thresholds, and fixture-based tests.
2. Add current-worktree and confirmed multi-worktree cleanup recipes.
3. Add warning-only preflights to high-growth `just` recipes.
4. Enable the separate `build-dir` as a compatibility canary on Linux, macOS,
   and Windows. Run scoped tests, then request approval for the complete
   workspace suite because this changes all Cargo builds.
5. Enable hard-limit failures after one week of warning-only observations.
6. Review the 40/80 GiB and 100/160 GiB defaults after 30 days.
7. Separately benchmark a 10 GiB `sccache`; do not gate the basic storage policy
   on that experiment.

## Success criteria

- A managed Clanker build cannot silently grow an already over-budget cache.
- `just target-size` shows aggregate linked-worktree target and build usage and
  measurement type.
- A contributor can safely reclaim the current worktree's build output with one
  command.
- Cleaning other worktrees requires an explicit, reviewable choice.
- Normal scoped test and lint workflows fit below the hard per-worktree limit.
- No linked build directory again reaches 300 GiB without repeated explicit
  overrides that are visible in command output.
