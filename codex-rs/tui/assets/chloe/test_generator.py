# Modified by Heiervang Technologies.
import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


GENERATOR = Path(__file__).with_name("generator.py")


class GeneratorTests(unittest.TestCase):
    def test_import_has_no_generation_side_effects(self):
        sheet = GENERATOR.parent.parent / "chloe-r2-09-locked-in" / "sheet.png"
        before = sheet.stat().st_mtime_ns
        spec = importlib.util.spec_from_file_location("chloe_generator", GENERATOR)
        module = importlib.util.module_from_spec(spec)
        self.assertIsNotNone(spec.loader)
        spec.loader.exec_module(module)
        self.assertEqual(sheet.stat().st_mtime_ns, before)

    def test_regeneration_matches_manifest_geometry(self):
        spec = importlib.util.spec_from_file_location("chloe_generator", GENERATOR)
        module = importlib.util.module_from_spec(spec)
        self.assertIsNotNone(spec.loader)
        spec.loader.exec_module(module)
        from PIL import Image

        with tempfile.TemporaryDirectory() as temp_dir:
            output_root = Path(temp_dir)
            source_dir = output_root / "chloe-r2-09"
            source_dir.mkdir()
            shutil.copy2(
                module.ASSET_ROOT / "chloe-r2-09" / "source.png",
                source_dir / "source.png",
            )
            module.save_variant(
                "09-locked-in",
                asset_root=output_root,
                preview_root=output_root,
            )

            sheet = output_root / "chloe-r2-09-locked-in" / "sheet.png"
            with Image.open(sheet) as image:
                self.assertEqual(
                    image.size,
                    (module.FRAME_SIZE * module.FRAME_COUNT, module.FRAME_SIZE),
                )

            manifest_path = (
                module.ASSET_ROOT / "chloe-r2-09-locked-in" / "avatar.json"
            )
            manifest = json.loads(manifest_path.read_text())
            frame = manifest["frame"]
            self.assertEqual(
                (frame["width"] * frame["columns"], frame["height"] * frame["rows"]),
                (module.FRAME_SIZE * module.FRAME_COUNT, module.FRAME_SIZE),
            )
            for animation in manifest["animations"].values():
                self.assertTrue(animation["frames"])
                self.assertTrue(
                    all(index < module.FRAME_COUNT for index in animation["frames"])
                )


if __name__ == "__main__":
    unittest.main()
