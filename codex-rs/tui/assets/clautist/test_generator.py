#!/usr/bin/env python3
"""Tests for the Clautist generator.

The tests that matter here are the ones about what the tool *cannot* do. A
preview generator earns its name by being unable to install, and "I did not
add an install path" is not evidence — a default output directory, a `..` in a
user-supplied path, or a symlink is enough to turn a preview into an overwrite.
So the guard is tested from several directions, including the ones I would not
have thought to type by hand.
"""
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

GEN = Path(__file__).with_name("generator.py")
HERE = GEN.parent


def load():
    spec = importlib.util.spec_from_file_location("clautist_generator", GEN)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


gen = load()


class ImportIsInert(unittest.TestCase):
    def test_importing_writes_nothing(self):
        sheet = HERE / "sheet.png"
        before = sheet.stat().st_mtime_ns
        load()
        self.assertEqual(sheet.stat().st_mtime_ns, before)


class LegacyCapture(unittest.TestCase):
    """(A) is a capture and its whole job is to notice drift."""

    def test_capture_matches_the_shipped_sheet(self):
        im = gen.legacy_capture()
        self.assertEqual(im.size, (gen.FRAME * gen.FRAME_COUNT, gen.FRAME))

    def test_capture_refuses_a_sheet_that_has_drifted(self):
        real = gen.LEGACY_SHA256
        try:
            gen.LEGACY_SHA256 = "0" * 64
            with self.assertRaises(SystemExit) as ctx:
                gen.legacy_capture()
            self.assertIn("MISMATCH", str(ctx.exception))
        finally:
            gen.LEGACY_SHA256 = real

    def test_capture_agrees_with_the_manifest(self):
        m = json.loads((HERE / "avatar.json").read_text())
        fr = m["frame"]
        self.assertEqual(
            (fr["width"], fr["height"], fr["columns"], fr["rows"]),
            (gen.FRAME, gen.FRAME, gen.FRAME_COUNT, 1),
        )


class PreviewCannotInstall(unittest.TestCase):
    """The approval seam, tested as a seam."""

    def test_output_dir_is_required_with_no_default(self):
        p = subprocess.run(
            [sys.executable, str(GEN), "preview"], capture_output=True, text=True
        )
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("--out", p.stderr)

    def test_refuses_the_asset_directory(self):
        with self.assertRaises(SystemExit):
            gen.guard_output_dir(HERE)

    def test_refuses_the_assets_parent(self):
        with self.assertRaises(SystemExit):
            gen.guard_output_dir(HERE.parent)

    def test_refuses_the_installed_avatar_root(self):
        with self.assertRaises(SystemExit):
            gen.guard_output_dir(Path.home() / ".codex" / "avatars")

    def test_refuses_a_traversal_back_into_the_asset_tree(self):
        # The check must run on the resolved path, or `safe/../clautist` walks
        # straight through it.
        with self.assertRaises(SystemExit):
            gen.guard_output_dir(HERE / ".." / HERE.name)

    def test_refuses_a_symlink_pointing_into_the_asset_tree(self):
        """The case that actually requires resolving the path.

        My `..` traversal test passed even with resolution removed — the
        unnormalised path still happens to carry the asset dir among its
        parents, so it caught the mutant by luck rather than by testing the
        thing. A symlink does not: without resolution the guard sees an
        innocent temp path and waves it through, straight into live assets.
        """
        with tempfile.TemporaryDirectory() as td:
            link = Path(td) / "innocent-looking"
            link.symlink_to(HERE, target_is_directory=True)
            with self.assertRaises(SystemExit):
                gen.guard_output_dir(link)

    def test_refuses_an_ancestor_of_the_asset_tree(self):
        with self.assertRaises(SystemExit):
            gen.guard_output_dir(Path.home())

    def test_allows_an_unrelated_directory(self):
        with tempfile.TemporaryDirectory() as td:
            out = gen.guard_output_dir(Path(td) / "preview")
            self.assertTrue(out.is_dir())

    def test_a_full_preview_run_leaves_the_live_sheet_untouched(self):
        sheet = HERE / "sheet.png"
        before = sheet.read_bytes()
        with tempfile.TemporaryDirectory() as td:
            gen.main(["preview", "--out", td])
            produced = {p.name for p in Path(td).iterdir()}
            self.assertIn("sheet-normalized-candidate.png", produced)
            # The candidate must not be named like an installable asset.
            self.assertNotIn("sheet.png", produced)
            self.assertNotIn("avatar.json", produced)
        self.assertEqual(sheet.read_bytes(), before)


class NormalizedOutput(unittest.TestCase):
    def test_one_palette_only(self):
        """(B)'s entire claim: no export drift, one authored palette.

        The shipped sheet carries five different inks for the same logical
        colour. The candidate must carry exactly one.
        """
        strip = gen.to_strip(gen.normalized_frames())
        colours = {
            strip.getpixel((x, y))
            for y in range(gen.FRAME)
            for x in range(gen.FRAME * gen.FRAME_COUNT)
            if strip.getpixel((x, y))[3]
        }
        # Dimmed tiers are a deliberate uniform scale of the same palette, so
        # allow them, but the count must stay far below the legacy 50.
        self.assertLess(len(colours), 25, sorted(colours))

    def test_alpha_is_binary(self):
        strip = gen.to_strip(gen.normalized_frames())
        alphas = set(strip.tobytes()[3::4])
        self.assertTrue(alphas <= {0, 255}, alphas - {0, 255})

    def test_frame_count_and_geometry(self):
        strip = gen.to_strip(gen.normalized_frames())
        self.assertEqual(strip.size, (gen.FRAME * gen.FRAME_COUNT, gen.FRAME))

    def test_every_frame_draws_something(self):
        for i, f in enumerate(gen.normalized_frames()):
            opaque = sum(1 for a in f.tobytes()[3::4] if a >= 128)
            self.assertGreater(opaque, 0, f"frame {i} is empty")

    def test_no_two_frames_are_identical(self):
        frames = [f.tobytes() for f in gen.to_strip(gen.normalized_frames()).split()[0:1]]
        strip = gen.to_strip(gen.normalized_frames())
        seen = {}
        for i in range(gen.FRAME_COUNT):
            key = strip.crop((i * gen.FRAME, 0, (i + 1) * gen.FRAME, gen.FRAME)).tobytes()
            self.assertNotIn(key, seen, f"frame {i} duplicates frame {seen.get(key)}")
            seen[key] = i

    def test_talking_moves_like_the_corpus_does(self):
        """Measured over 94 talking frames in the shipped sets: median 4% of
        the character moves, and the largest legitimate one is 35%. The shipped
        Clautist frames move 100% and strobe. Keep the candidate in range."""
        strip = gen.to_strip(gen.normalized_frames())

        def frame(i):
            return strip.crop((i * gen.FRAME, 0, (i + 1) * gen.FRAME, gen.FRAME))

        def footprint(im):
            a = im.tobytes()[3::4]
            return {(i % gen.FRAME, i // gen.FRAME) for i, v in enumerate(a) if v >= 128}

        base = frame(0)
        for idx in (20, 21):
            cur = frame(idx)
            diff = sum(
                1
                for y in range(gen.FRAME)
                for x in range(gen.FRAME)
                if base.getpixel((x, y)) != cur.getpixel((x, y))
            )
            share = diff * 100 // len(footprint(base) | footprint(cur))
            self.assertGreater(share, 5, f"talking frame {idx} will not be visible")
            self.assertLessEqual(share, 40, f"talking frame {idx} is a pose change")


class Cli(unittest.TestCase):
    def test_verify_exits_clean(self):
        p = subprocess.run(
            [sys.executable, str(GEN), "verify"], capture_output=True, text=True
        )
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_there_is_no_install_subcommand(self):
        p = subprocess.run(
            [sys.executable, str(GEN), "install"], capture_output=True, text=True
        )
        self.assertNotEqual(p.returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
