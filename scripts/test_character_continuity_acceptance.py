#!/usr/bin/env python3

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import character_continuity_acceptance as acceptance


class FixtureTests(unittest.TestCase):
    def test_character_avatar_references_are_complete_fixture_packs(self) -> None:
        registries = (
            "valid_registry",
            "collision_registry",
            "canonical_collision_registry",
        )

        for registry in registries:
            characters = acceptance.FIXTURES / registry / "characters"
            for character_path in characters.glob("*/character.json"):
                character = json.loads(character_path.read_text(encoding="utf-8"))
                avatar_refs = [character["avatar"]]
                avatar_refs.extend(character.get("avatarByMode", {}).values())
                for avatar_ref in avatar_refs:
                    avatar_path = character_path.parent / avatar_ref
                    self.assertTrue(avatar_path.is_file(), avatar_path)
                    avatar = json.loads(avatar_path.read_text(encoding="utf-8"))
                    self.assertEqual(avatar["renderMode"], "ansi-half-block")
                    spritesheet = avatar_path.parent / avatar["spritesheetPath"]
                    self.assertTrue(spritesheet.is_file(), spritesheet)
                    ppm = spritesheet.read_text(encoding="ascii").split()
                    self.assertEqual(ppm[:4], ["P3", "24", "24", "255"])
                    pixels = [int(sample) for sample in ppm[4:]]
                    self.assertEqual(len(pixels), 24 * 24 * 3)
                    self.assertTrue(all(0 <= sample <= 255 for sample in pixels))
                    self.assertEqual(
                        avatar["frame"],
                        {
                            "width": 24,
                            "height": 24,
                            "columns": 1,
                            "rows": 1,
                        },
                    )


class EvidenceTests(unittest.TestCase):
    def load_fixture(self, name: str) -> dict[str, object]:
        path = acceptance.FIXTURES / name
        return json.loads(path.read_text(encoding="utf-8"))

    def test_passing_evidence_satisfies_lifecycle_and_pet_contracts(self) -> None:
        payload = self.load_fixture("passing_evidence.json")

        self.assertIsNone(acceptance.validate_lifecycle_evidence(payload))
        self.assertIsNone(acceptance.validate_pet_evidence(payload))

    def test_baseline_evidence_records_both_known_failures(self) -> None:
        payload = self.load_fixture("baseline_evidence.json")

        self.assertEqual(
            acceptance.validate_lifecycle_evidence(payload),
            "Director mismatch for searching",
        )
        self.assertEqual(
            acceptance.validate_pet_evidence(payload),
            "avatarId must remain stable when pet selection changes",
        )


class HarnessTests(unittest.TestCase):
    def make_binary(self, directory: Path, *, ready: bool) -> Path:
        path = directory / "clanker-fixture"
        implementation = """#!/usr/bin/env python3
import json
import pathlib
import os
import sys

args = sys.argv[1:]
ready = READY
if args == ["--help"]:
    print("Clanker fixture" + (" --name NAME" if ready else ""))
    raise SystemExit(0)
if not ready or len(args) < 2 or args[0] != "character":
    raise SystemExit(2)
command = args[1]
if command == "validate":
    if "--all" in args:
        canonical = "canonical-collision" in os.environ.get("CODEX_HOME", "")
        if canonical:
            errors = [{"code": "canonical_collision", "message": "collision", "conflictKind": "canonical_vs_canonical"}]
        else:
            errors = [
                {"code": "alias_collision", "message": "collision", "conflictKind": "alias_vs_alias"},
                {"code": "alias_collision", "message": "collision", "conflictKind": "alias_vs_canonical"},
            ]
        body = {"schemaVersion": 1, "ok": False, "errors": errors, "warnings": []}
        print(json.dumps(body))
        raise SystemExit(1)
    manifest = pathlib.Path(args[2])
    data = json.loads(manifest.read_text())
    if "avatar" not in data:
        body = {"schemaVersion": 1, "ok": False, "manifestPath": str(manifest), "id": data.get("id"), "errors": [{"code": "missing_avatar", "message": "missing avatar"}], "warnings": []}
        print(json.dumps(body))
        raise SystemExit(1)
    body = {"schemaVersion": 1, "ok": True, "manifestPath": str(manifest), "id": data["id"], "errors": [], "warnings": []}
    print(json.dumps(body))
    raise SystemExit(0)
if command == "resolve":
    if args == ["character", "resolve", "--json"]:
        print("Usage: clanker character resolve NAME_OR_ALIAS --json", file=sys.stderr)
        raise SystemExit(2)
    value = args[2]
    matches = {"c3ph0": "exact_canonical", "C3PH0": "casefold_canonical", "c3p-h0": "explicit_alias"}
    if value not in matches:
        print(json.dumps({"schemaVersion": 1, "ok": False, "errors": [{"code": "not_found", "message": "not found"}]}))
        raise SystemExit(1)
    print(json.dumps({"schemaVersion": 1, "ok": True, "input": value, "id": "c3ph0", "displayName": "C3PH0", "manifestPath": "/fixture/c3ph0/character.json", "matchKind": matches[value]}))
    raise SystemExit(0)
raise SystemExit(2)
""".replace("READY", "True" if ready else "False")
        path.write_text(implementation, encoding="utf-8")
        path.chmod(0o755)
        return path

    def run_harness(self, *, ready: bool, mode: str) -> list[acceptance.CheckResult]:
        with tempfile.TemporaryDirectory() as temp:
            binary = self.make_binary(Path(temp), ready=ready)
            return acceptance.Harness(str(binary), mode, None, None).run()

    def test_acceptance_mode_passes_ready_executable(self) -> None:
        results = self.run_harness(ready=True, mode="acceptance")

        self.assertTrue(results)
        self.assertEqual({result.status for result in results}, {"pass"})

    def test_baseline_mode_classifies_known_contract_gaps(self) -> None:
        results = self.run_harness(ready=False, mode="baseline")

        self.assertTrue(results)
        self.assertEqual({result.status for result in results}, {"known_failure"})

    def test_acceptance_mode_rejects_baseline_executable(self) -> None:
        results = self.run_harness(ready=False, mode="acceptance")

        self.assertTrue(results)
        self.assertEqual({result.status for result in results}, {"fail"})


if __name__ == "__main__":
    unittest.main()
