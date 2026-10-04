#!/usr/bin/env python3
"""Apache-2.0 section 4(b) modification notices for upstream-derived files.

The License requires that "any modified files carry prominent notices stating
that You changed the files". This repository is a downstream distribution of
openai/codex, so every upstream-derived file the fork has changed needs such a
notice, and the set of those files has to be derived mechanically rather than
maintained by hand.

The inventory is a tree diff against the pinned upstream baseline recorded in
`.github/upstream-baseline`. A pinned baseline is deliberate: `main` is a
moving mirror of upstream, and diffing against a moving ref would attribute
later upstream edits to the fork. Bump the baseline as part of a fork sync,
then re-run `--apply`.

Note that the `clanker` branch has no merge base with `main` (it starts from an
orphan "initial open-source release" commit), so `git merge-base` cannot be
used here. `git diff <baseline> HEAD` is a tree comparison and works anyway.

Usage:
    verify_modification_notices.py --check   # CI gate; non-zero if any file is missing its notice
    verify_modification_notices.py --list    # print the inventory and exit
    verify_modification_notices.py --apply   # insert missing notices in place
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BASELINE_FILE = REPO_ROOT / ".github" / "upstream-baseline"

# Stable marker. Detection matches on this substring only, so the surrounding
# wording can be revised without invalidating every already-annotated file.
NOTICE_MARKER = "Modified by Heiervang Technologies"
NOTICE_TEXT = (
    f"{NOTICE_MARKER} from the openai/codex original; see NOTICE for fork provenance."
)

# Comment syntax by extension. A file type absent from both maps is skipped:
# we only annotate files where a notice can be expressed as a real comment.
LINE_COMMENT_PREFIXES = {
    ".rs": "//",
    ".ts": "//",
    ".tsx": "//",
    ".js": "//",
    ".mjs": "//",
    ".bzl": "#",
    ".bazel": "#",
    ".py": "#",
    ".sh": "#",
    ".toml": "#",
    ".yml": "#",
    ".yaml": "#",
}
BLOCK_COMMENT_EXTENSIONS = {".md": ("<!-- ", " -->")}

# Filenames without a useful suffix that still take a `#` comment.
LINE_COMMENT_FILENAMES = {
    "justfile": "#",
    "Justfile": "#",
    "Dockerfile": "#",
    "MODULE.bazel": "#",
    "BUILD.bazel": "#",
}

# Generated or machine-owned files. A modification notice in a lockfile or an
# insta snapshot would be rewritten by the tool that owns the file, so the
# notice would not survive and the check would flap.
EXCLUDED_GLOBS = (
    "*.lock",
    "*.snap",
    "*.json",
    "*.png",
    "*.jpg",
    "*.svg",
    "*.ico",
    "**/vendor/**",
    "**/node_modules/**",
    "**/snapshots/**",
    "**/*.snap.new",
    "pnpm-lock.yaml",
    "MODULE.bazel.lock",
)


SHEBANG_RE = re.compile(r"^#!\s*/")


class BaselineError(RuntimeError):
    """The pinned upstream baseline is missing or not present in this clone."""


@dataclass(frozen=True)
class Annotatable:
    path: Path
    rel: str
    opener: str
    closer: str

    def notice_line(self) -> str:
        return f"{self.opener}{NOTICE_TEXT}{self.closer}"


def run_git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        check=True,
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    return result.stdout


def read_baseline() -> str:
    if not BASELINE_FILE.exists():
        raise BaselineError(
            f"{BASELINE_FILE.relative_to(REPO_ROOT)} is missing; it must pin the "
            "upstream commit this fork's modifications are measured against."
        )
    for line in BASELINE_FILE.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            return line
    raise BaselineError(
        f"{BASELINE_FILE.relative_to(REPO_ROOT)} contains no commit SHA."
    )


def ensure_baseline_present(baseline: str) -> None:
    """Fail loudly rather than silently inventorying nothing.

    A shallow CI checkout will not have the baseline object. Reporting an empty
    inventory in that case would make the gate pass for the wrong reason, so
    treat a missing object as an error and tell the caller how to fix it.
    """
    try:
        run_git("cat-file", "-e", f"{baseline}^{{commit}}")
        return
    except subprocess.CalledProcessError:
        pass
    try:
        run_git("fetch", "--no-tags", "--depth=1", "origin", baseline)
        run_git("cat-file", "-e", f"{baseline}^{{commit}}")
    except subprocess.CalledProcessError as exc:
        raise BaselineError(
            f"upstream baseline {baseline} is not present in this clone and could "
            "not be fetched. Check out with `fetch-depth: 0`, or fetch the "
            "baseline commit before running this check."
        ) from exc


def is_excluded(rel: str) -> bool:
    path = Path(rel)
    for pattern in EXCLUDED_GLOBS:
        if fnmatch.fnmatch(rel, pattern) or fnmatch.fnmatch(path.name, pattern):
            return True
        if "**" in pattern and fnmatch.fnmatch(f"/{rel}", f"/{pattern}"):
            return True
    return False


def comment_style(rel: str) -> tuple[str, str] | None:
    path = Path(rel)
    if path.name in LINE_COMMENT_FILENAMES:
        return f"{LINE_COMMENT_FILENAMES[path.name]} ", ""
    suffix = path.suffix
    if suffix in LINE_COMMENT_PREFIXES:
        return f"{LINE_COMMENT_PREFIXES[suffix]} ", ""
    if suffix in BLOCK_COMMENT_EXTENSIONS:
        return BLOCK_COMMENT_EXTENSIONS[suffix]
    return None


def modified_upstream_files(baseline: str) -> list[str]:
    """Files that exist upstream at the baseline and differ on this branch.

    Only `M` status counts. Added files are fork-owned originals rather than
    modifications of an upstream work, and deleted files carry nothing.
    """
    out = run_git("diff", "--name-only", "--diff-filter=M", baseline, "HEAD")
    return [line for line in out.splitlines() if line]


def inventory(baseline: str) -> list[Annotatable]:
    entries: list[Annotatable] = []
    for rel in modified_upstream_files(baseline):
        if is_excluded(rel):
            continue
        style = comment_style(rel)
        if style is None:
            continue
        path = REPO_ROOT / rel
        if not path.is_file():
            continue
        entries.append(Annotatable(path=path, rel=rel, opener=style[0], closer=style[1]))
    return sorted(entries, key=lambda e: e.rel)


def has_notice(entry: Annotatable) -> bool:
    try:
        text = entry.path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return True  # Binary-ish; nothing sensible to insert.
    return NOTICE_MARKER in text


def insertion_index(lines: list[str], rel: str) -> int:
    """Where the notice goes: the top, except where the top is reserved.

    A shebang must stay on line 1, and a YAML document start marker must stay
    ahead of document content. Everything else takes the notice at line 1 so it
    is the first thing a reader sees, which is what "prominent" asks for.

    `#!` is not enough to identify a shebang: a Rust inner attribute such as
    `#![cfg(...)]` starts the same way, and comments may legally precede it, so
    those files take the notice at the very top like everything else.
    """
    if lines and SHEBANG_RE.match(lines[0]):
        return 1
    if Path(rel).suffix in (".yml", ".yaml") and lines and lines[0].strip() == "---":
        return 1
    return 0


def apply_notice(entry: Annotatable) -> None:
    text = entry.path.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    index = insertion_index(lines, entry.rel)
    block = [entry.notice_line() + "\n"]
    # Keep a blank separator unless the file already starts one.
    if index < len(lines) and lines[index].strip():
        block.append("\n")
    lines[index:index] = block
    entry.path.write_text("".join(lines), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="fail if a notice is missing")
    mode.add_argument("--list", action="store_true", help="print the inventory")
    mode.add_argument("--apply", action="store_true", help="insert missing notices")
    args = parser.parse_args()

    try:
        baseline = read_baseline()
        ensure_baseline_present(baseline)
    except BaselineError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    entries = inventory(baseline)

    if args.list:
        for entry in entries:
            print(entry.rel)
        print(f"\n{len(entries)} upstream-derived file(s) modified by this fork.")
        return 0

    missing = [entry for entry in entries if not has_notice(entry)]

    if args.apply:
        for entry in missing:
            apply_notice(entry)
        print(f"Added modification notices to {len(missing)} file(s).")
        return 0

    if missing:
        print(
            "Apache-2.0 section 4(b): the following upstream-derived files were "
            "modified by this fork but carry no modification notice:\n",
            file=sys.stderr,
        )
        for entry in missing:
            print(f"  {entry.rel}", file=sys.stderr)
        print(
            f"\n{len(missing)} of {len(entries)} file(s) missing a notice. "
            "Run `python3 .github/scripts/verify_modification_notices.py --apply`.",
            file=sys.stderr,
        )
        return 1

    print(f"All {len(entries)} modified upstream-derived file(s) carry a notice.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
