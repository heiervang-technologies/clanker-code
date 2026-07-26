#!/usr/bin/env python3
# Modified by Heiervang Technologies.
"""Black-box acceptance harness for the Character Continuity executable contract."""

from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Callable

SCHEMA_VERSION = 1
MATCH_KINDS = {
    "exact_canonical",
    "casefold_canonical",
    "explicit_alias",
}
BASELINE_FAILURES = {
    "launch.direct_name_flag_wiring",
    "character.validate.valid",
    "character.validate.missing_avatar",
    "character.validate.alias_collision",
    "character.validate.canonical_collision",
    "character.resolve.exact",
    "character.resolve.casefold",
    "character.resolve.alias",
    "character.resolve.not_found",
    "character.resolve.usage",
    "launch.unleash_explicit_name",
    "evidence.lifecycle",
    "evidence.pet_independence",
}
FIXTURES = Path(__file__).with_name("character_continuity") / "fixtures"


@dataclass(frozen=True)
class CheckResult:
    name: str
    status: str
    detail: str


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str


def run_command(argv: list[str], env: dict[str, str]) -> CommandResult:
    try:
        completed = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            env=env,
            text=True,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return CommandResult(127, "", str(error))
    return CommandResult(completed.returncode, completed.stdout, completed.stderr)


def parse_json_result(result: CommandResult) -> tuple[dict[str, Any] | None, str]:
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        return None, f"stdout is not JSON: {error.msg}"
    if not isinstance(payload, dict):
        return None, "JSON response must be an object"
    if payload.get("schemaVersion") != SCHEMA_VERSION:
        return None, "schemaVersion must equal 1"
    if not isinstance(payload.get("ok"), bool):
        return None, "ok must be a boolean"
    return payload, ""


def validate_success(
    result: CommandResult,
    *,
    expected_id: str,
    match_kind: str | None = None,
    expected_input: str | None = None,
) -> str | None:
    if result.returncode != 0:
        return f"expected exit 0, got {result.returncode}: {result.stderr.strip()}"
    payload, error = parse_json_result(result)
    if payload is None:
        return error
    if payload.get("ok") is not True or payload.get("id") != expected_id:
        return f"expected ok=true and canonical id={expected_id}"
    if not isinstance(payload.get("manifestPath"), str):
        return "manifestPath must be a string"
    if match_kind is not None:
        if payload.get("matchKind") != match_kind or match_kind not in MATCH_KINDS:
            return f"expected matchKind={match_kind}"
        if payload.get("input") != expected_input:
            return f"expected input to round-trip as {expected_input!r}"
        if not isinstance(payload.get("displayName"), str):
            return "displayName must be a string"
    else:
        if not isinstance(payload.get("errors"), list):
            return "validation errors must be an array"
        if not isinstance(payload.get("warnings"), list):
            return "validation warnings must be an array"
    return None


def validate_semantic_failure(
    result: CommandResult,
    expected_code: str,
    expected_conflict_kinds: list[str] | None = None,
) -> str | None:
    if result.returncode != 1:
        return f"expected semantic exit 1, got {result.returncode}"
    payload, error = parse_json_result(result)
    if payload is None:
        return error
    if payload.get("ok") is not False:
        return "semantic failure must return ok=false"
    errors = payload.get("errors")
    if not isinstance(errors, list):
        return "semantic failure errors must be an array"
    for item in errors:
        if not isinstance(item, dict):
            return "semantic failure errors must contain objects"
        if not isinstance(item.get("code"), str) or not isinstance(
            item.get("message"), str
        ):
            return "semantic failure errors require string code and message"
        if "path" in item and not isinstance(item["path"], str):
            return "semantic failure error path must be a string"
    codes = {item.get("code") for item in errors}
    if expected_code not in codes:
        return f"expected error code {expected_code}, got {sorted(codes - {None})}"
    if expected_conflict_kinds is not None:
        conflict_kinds = [
            item.get("conflictKind")
            for item in errors
            if item.get("code") == expected_code
        ]
        if conflict_kinds != expected_conflict_kinds:
            return (
                f"expected ordered conflict kinds {expected_conflict_kinds}, "
                f"got {conflict_kinds}"
            )
    return None


def validate_lifecycle_evidence(payload: dict[str, Any]) -> str | None:
    if payload.get("schemaVersion") != SCHEMA_VERSION:
        return "lifecycle evidence schemaVersion must equal 1"
    expected_avatar = {
        "analyzing": {"planning", "running"},
        "searching": {"running"},
        "responding": {"running"},
        "talking": {"talking"},
        "listening": {"idle"},
        "idle": {"idle"},
    }
    director_states = {"idle", "waiting", "working", "rate-limited", "offline"}
    observations = payload.get("lifecycle")
    if not isinstance(observations, list) or not observations:
        return "lifecycle evidence must contain observations"
    for observation in observations:
        if not isinstance(observation, dict):
            return "lifecycle observation must be an object"
        state = observation.get("agentState")
        if state not in expected_avatar:
            return f"unknown agentState {state!r}"
        if observation.get("avatarState") not in expected_avatar[state]:
            return f"avatar mismatch for {state}"
        director_state = observation.get("directorState")
        if director_state not in director_states:
            return f"invalid Director state {director_state!r}"
        if (
            state in {"analyzing", "searching", "responding"}
            and director_state != "working"
        ):
            return f"Director mismatch for {state}"
    if observations[-1].get("agentState") not in {"listening", "idle"}:
        return "final lifecycle state must settle to listening or idle"
    if observations[-1].get("avatarState") != "idle":
        return "final avatar state must settle to idle"
    if observations[-1].get("directorState") != "idle":
        return "final Director state must settle to idle"
    return None


def validate_pet_evidence(payload: dict[str, Any]) -> str | None:
    if payload.get("schemaVersion") != SCHEMA_VERSION:
        return "pet evidence schemaVersion must equal 1"
    evidence = payload.get("petIndependence")
    if not isinstance(evidence, dict):
        return "petIndependence evidence must be an object"
    before = evidence.get("before")
    after = evidence.get("after")
    if not isinstance(before, dict) or not isinstance(after, dict):
        return "pet evidence requires before and after objects"
    for key in (
        "clankerId",
        "avatarId",
        "avatarPlacement",
        "collaborationMode",
        "activeVariant",
        "avatarState",
    ):
        if not before.get(key) or before.get(key) != after.get(key):
            return f"{key} must remain stable when pet selection changes"
    if before.get("petId") == after.get("petId"):
        return "petId must change in the independence probe"
    return None


class Harness:
    def __init__(
        self,
        binary: str,
        mode: str,
        unleash: str | None,
        evidence: Path | None,
    ) -> None:
        self.binary = binary
        self.mode = mode
        self.unleash = unleash
        self.evidence = evidence

    def run(self) -> list[CheckResult]:
        with tempfile.TemporaryDirectory(prefix="clanker-character-contract-") as temp:
            temp_root = Path(temp)
            valid_home = temp_root / "valid-home"
            collision_home = temp_root / "collision-home"
            canonical_collision_home = temp_root / "canonical-collision-home"
            shutil.copytree(FIXTURES / "valid_registry", valid_home)
            shutil.copytree(FIXTURES / "collision_registry", collision_home)
            shutil.copytree(
                FIXTURES / "canonical_collision_registry", canonical_collision_home
            )
            return self._run_cases(valid_home, collision_home, canonical_collision_home)

    def _run_cases(
        self,
        valid_home: Path,
        collision_home: Path,
        canonical_collision_home: Path,
    ) -> list[CheckResult]:
        valid_env = {**os.environ, "CODEX_HOME": str(valid_home)}
        collision_env = {**os.environ, "CODEX_HOME": str(collision_home)}
        canonical_collision_env = {
            **os.environ,
            "CODEX_HOME": str(canonical_collision_home),
        }
        manifest = valid_home / "characters/c3ph0/character.json"
        missing_avatar = FIXTURES / "invalid_missing_avatar.json"
        cases: list[tuple[str, Callable[[], str | None]]] = [
            (
                "launch.direct_name_flag_wiring",
                lambda: self._check_name_help(valid_env),
            ),
            (
                "character.validate.valid",
                lambda: validate_success(
                    run_command(
                        [self.binary, "character", "validate", str(manifest), "--json"],
                        valid_env,
                    ),
                    expected_id="c3ph0",
                ),
            ),
            (
                "character.validate.missing_avatar",
                lambda: validate_semantic_failure(
                    run_command(
                        [
                            self.binary,
                            "character",
                            "validate",
                            str(missing_avatar),
                            "--json",
                        ],
                        valid_env,
                    ),
                    "missing_avatar",
                ),
            ),
            (
                "character.validate.alias_collision",
                lambda: validate_semantic_failure(
                    run_command(
                        [self.binary, "character", "validate", "--all", "--json"],
                        collision_env,
                    ),
                    "alias_collision",
                    ["alias_vs_alias", "alias_vs_canonical"],
                ),
            ),
            (
                "character.validate.canonical_collision",
                lambda: validate_semantic_failure(
                    run_command(
                        [self.binary, "character", "validate", "--all", "--json"],
                        canonical_collision_env,
                    ),
                    "canonical_collision",
                    ["canonical_vs_canonical"],
                ),
            ),
        ]
        for name, value, match_kind in (
            ("exact", "c3ph0", "exact_canonical"),
            ("casefold", "C3PH0", "casefold_canonical"),
            ("alias", "c3p-h0", "explicit_alias"),
        ):
            cases.append(
                (
                    f"character.resolve.{name}",
                    lambda value=value, match_kind=match_kind: validate_success(
                        run_command(
                            [self.binary, "character", "resolve", value, "--json"],
                            valid_env,
                        ),
                        expected_id="c3ph0",
                        match_kind=match_kind,
                        expected_input=value,
                    ),
                )
            )
        cases.append(
            (
                "character.resolve.not_found",
                lambda: validate_semantic_failure(
                    run_command(
                        [self.binary, "character", "resolve", "unknown", "--json"],
                        valid_env,
                    ),
                    "not_found",
                ),
            )
        )
        cases.append(
            (
                "character.resolve.usage",
                lambda: self._check_resolve_usage(valid_env),
            )
        )
        if self.unleash:
            cases.extend(
                [
                    (
                        "launch.unleash_bare",
                        lambda: self._check_unleash(valid_env, explicit=False),
                    ),
                    (
                        "launch.unleash_explicit_name",
                        lambda: self._check_unleash(valid_env, explicit=True),
                    ),
                ]
            )
        if self.evidence:
            payload = json.loads(self.evidence.read_text(encoding="utf-8"))
            cases.extend(
                [
                    (
                        "evidence.lifecycle",
                        lambda: validate_lifecycle_evidence(payload),
                    ),
                    (
                        "evidence.pet_independence",
                        lambda: validate_pet_evidence(payload),
                    ),
                ]
            )
        return [self._evaluate(name, check()) for name, check in cases]

    def _evaluate(self, name: str, error: str | None) -> CheckResult:
        if error is None:
            return CheckResult(name, "pass", "contract satisfied")
        if self.mode == "baseline" and name in BASELINE_FAILURES:
            return CheckResult(name, "known_failure", error)
        return CheckResult(name, "fail", error)

    def _check_name_help(self, env: dict[str, str]) -> str | None:
        result = run_command([self.binary, "--help"], env)
        if result.returncode != 0:
            return f"--help exited {result.returncode}"
        if "--name" not in result.stdout:
            return "root CLI does not expose --name"
        return None

    def _check_resolve_usage(self, env: dict[str, str]) -> str | None:
        result = run_command(
            [self.binary, "character", "resolve", "--json"],
            env,
        )
        if result.returncode != 2:
            return f"missing resolve input must exit 2, got {result.returncode}"
        usage_lines = [
            line.lower()
            for line in f"{result.stdout}\n{result.stderr}".splitlines()
            if line.lower().startswith("usage:")
        ]
        if not any(" character resolve " in f"{line} " for line in usage_lines):
            return "usage output must identify the character resolve command"
        return None

    def _check_unleash(self, env: dict[str, str], *, explicit: bool) -> str | None:
        argv = [self.unleash or "unleash", "clanker"]
        if explicit:
            argv.extend(["--name", "c3ph0"])
        argv.append("--dry-run")
        result = run_command(argv, env)
        if result.returncode != 0:
            return f"unleash dry-run exited {result.returncode}"
        line = next(
            (
                line
                for line in result.stdout.splitlines()
                if line.startswith("Would execute:")
            ),
            "",
        )
        command = shlex.split(line.removeprefix("Would execute:").strip())
        if not command or Path(command[0]).name != "clanker":
            return "unleash did not resolve the clanker executable"
        has_name = any(
            command[index : index + 2] == ["--name", "c3ph0"]
            for index in range(len(command) - 1)
        )
        if explicit and not has_name:
            return "unleash dropped explicit --name c3ph0"
        if not explicit and has_name:
            return "bare unleash launch injected an explicit identity"
        return None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="clanker")
    parser.add_argument("--unleash", nargs="?", const="unleash")
    parser.add_argument(
        "--mode", choices=("baseline", "acceptance"), default="acceptance"
    )
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--json", action="store_true", dest="json_output")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    results = Harness(args.binary, args.mode, args.unleash, args.evidence).run()
    if args.json_output:
        print(
            json.dumps(
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "ok": all(result.status != "fail" for result in results),
                    "mode": args.mode,
                    "results": [asdict(result) for result in results],
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        for result in results:
            print(f"{result.status.upper():13} {result.name}: {result.detail}")
    return 1 if any(result.status == "fail" for result in results) else 0


if __name__ == "__main__":
    sys.exit(main())
