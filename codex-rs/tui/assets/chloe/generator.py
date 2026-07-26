#!/usr/bin/env python3
# Modified by Heiervang Technologies.
"""Build the Chloe R2 ANSI avatar candidates from their hand-rolled sources.

The source silhouettes stay fixed. Animation is limited to facial reflections,
signal accents, status shading, and mouth pixels so the authored hair length and
shape survive every state.
"""

from pathlib import Path
import sys

from PIL import Image


ASSET_ROOT = Path(__file__).resolve().parent.parent
FRAME_SIZE = 24
FRAME_COUNT = 22

TRANSPARENT = (0, 0, 0, 0)
INK = (4, 8, 7, 255)
GLINT = (255, 241, 232, 255)
GLINT_DIM = (111, 139, 127, 255)
LENS_TINT = (109, 170, 44, 255)
LENS_DIM = (55, 86, 24, 255)
SIGNAL = (74, 255, 82, 255)
SIGNAL_DIM = (18, 103, 39, 255)
GREEN = (71, 235, 89, 255)
GREEN_DIM = (30, 151, 57, 255)
RED = (238, 78, 72, 255)
CYAN = (114, 238, 225, 255)
BANDANA = (218, 52, 62, 255)
BANDANA_HIGHLIGHT = (255, 101, 92, 255)
BANDANA_SHADOW = (112, 25, 38, 255)

VARIANTS = ("01", "09", "12", "09-locked-in")


def clone(source: Image.Image) -> Image.Image:
    return source.copy()


def pixels(image: Image.Image, points, color) -> Image.Image:
    for point in points:
        image.putpixel(point, color)
    return image


def lens_glint(
    source: Image.Image, phase: int, highlight: tuple[int, int, int, int] = GLINT
) -> Image.Image:
    image = clone(source)
    lens_pixels = ((8, 10), (9, 10), (14, 10), (15, 10))
    pixels(image, lens_pixels, LENS_TINT)
    highlights = (((8, 10), (15, 10)), ((9, 10), (14, 10)))[phase]
    return pixels(image, highlights, highlight)


def lens_dip(source: Image.Image) -> Image.Image:
    image = clone(source)
    return pixels(image, ((8, 10), (9, 10), (14, 10), (15, 10)), LENS_DIM)


def signal(source: Image.Image, points, color=SIGNAL) -> Image.Image:
    return pixels(clone(source), points, color)


def tint(source: Image.Image, replacements) -> Image.Image:
    image = clone(source)
    data = [
        replacements.get(image.getpixel((x, y)), image.getpixel((x, y)))
        for y in range(image.height)
        for x in range(image.width)
    ]
    image.putdata(data)
    return image


def mouth(source: Image.Image, open_amount: int) -> Image.Image:
    image = clone(source)
    if open_amount == 1:
        return pixels(image, ((11, 13), (12, 13)), INK)
    pixels(image, ((10, 13), (11, 13), (12, 13), (13, 13)), INK)
    return pixels(image, ((11, 14), (12, 14)), GLINT_DIM)


def locked_in_bandana(source: Image.Image) -> Image.Image:
    """Add a tied forehead band without touching the hair length or glasses."""
    image = clone(source)
    pixels(image, ((6, 7), (17, 7), (6, 8), (17, 8)), BANDANA_SHADOW)
    pixels(image, tuple((x, 7) for x in range(7, 17)), BANDANA)
    pixels(image, tuple((x, 8) for x in range(7, 17)), BANDANA)
    pixels(image, ((8, 7), (9, 7), (13, 7), (14, 7)), BANDANA_HIGHLIGHT)
    pixels(image, ((18, 7), (19, 8)), BANDANA)
    pixels(image, ((19, 7), (20, 8), (19, 9)), BANDANA_SHADOW)
    return image


def frames_for(source: Image.Image, variant: str) -> list[Image.Image]:
    # 0-2: idle. The long hold is encoded in avatar.json.
    idle = [clone(source), lens_glint(source, 1), lens_dip(source)]

    # 3-6: running. Data traces move around the fixed silhouette.
    running_points = (
        ((1, 8), (22, 15)),
        ((2, 6), (21, 17)),
        ((1, 15), (22, 8)),
        ((2, 17), (21, 6)),
    )
    running = [signal(source, points, SIGNAL_DIM) for points in running_points]

    # 7-8: waiting. Low-frequency side beacons.
    waiting = [
        signal(source, ((2, 11),), SIGNAL_DIM),
        signal(source, ((21, 11),), SIGNAL),
    ]

    # 9-10: review. A cyan scan moves across the sunglass lenses without
    # touching their dark frames.
    review = [
        pixels(lens_glint(source, 0), ((8, 10), (15, 10)), CYAN),
        pixels(lens_glint(source, 1), ((9, 10), (14, 10)), CYAN),
    ]

    # 11-12: failed. Hair, outline, and sunglass frames stay untouched; only
    # the signal accents and lens interiors go red.
    failed_base = tint(source, {GREEN: GREEN_DIM, SIGNAL: SIGNAL_DIM})
    failed = [
        pixels(clone(failed_base), ((8, 10), (15, 10)), RED),
        pixels(clone(failed_base), ((8, 10), (9, 10), (14, 10), (15, 10)), RED),
    ]

    # 13-15: planning. Each candidate keeps its own peripheral identity while
    # a three-step data orbit accrues around it.
    orbit = {
        "01": (((3, 10),), ((3, 10), (20, 7)), ((3, 10), (20, 7), (20, 17))),
        "09": (((3, 5),), ((3, 5), (21, 11)), ((3, 5), (21, 11), (20, 18))),
        "12": (((4, 4),), ((4, 4), (21, 11)), ((4, 4), (21, 11), (3, 18))),
    }[variant.removesuffix("-locked-in")]
    planning = [signal(source, points) for points in orbit]

    # 16-17: tired idle. Keep the sunglasses; only their reflections dim.
    tired_base = tint(source, {GLINT: GLINT_DIM, SIGNAL: SIGNAL_DIM})
    tired_idle = [clone(tired_base), lens_glint(tired_base, 1, GLINT_DIM)]

    # 18-19: tired running. Sparse traces, no bobbing or hair crop.
    tired_running = [
        signal(tired_base, ((1, 13),), SIGNAL_DIM),
        signal(tired_base, ((22, 13),), SIGNAL_DIM),
    ]

    # 20-21: talking. Closed mouth reuses frame 0 in the manifest.
    talking = [mouth(source, 1), mouth(source, 2)]
    frames = (
        idle
        + running
        + waiting
        + review
        + failed
        + planning
        + tired_idle
        + tired_running
        + talking
    )
    assert len(frames) == FRAME_COUNT
    return frames


def save_variant(
    variant: str,
    asset_root: Path = ASSET_ROOT,
    preview_root: Path = Path("/tmp"),
) -> None:
    directory = asset_root / f"chloe-r2-{variant}"
    directory.mkdir(parents=True, exist_ok=True)
    if variant == "09-locked-in":
        source = Image.open(asset_root / "chloe-r2-09" / "source.png").convert("RGBA")
        source = locked_in_bandana(source)
        source.save(directory / "source.png", optimize=True)
    else:
        source = Image.open(directory / "source.png").convert("RGBA")
    if source.size != (FRAME_SIZE, FRAME_SIZE):
        raise ValueError(f"{directory / 'source.png'} must be 24x24")

    frames = frames_for(source, variant)
    strip = Image.new(
        "RGBA", (FRAME_SIZE * FRAME_COUNT, FRAME_SIZE), TRANSPARENT
    )
    for index, frame in enumerate(frames):
        alpha = frame.getchannel("A").point(lambda value: 255 if value >= 128 else 0)
        frame.putalpha(alpha)
        strip.paste(frame, (index * FRAME_SIZE, 0))
    strip.save(directory / "sheet.png", optimize=True)

    preview_frames = [
        frames[index].resize((288, 288), Image.Resampling.NEAREST)
        for index in (0, 1, 0, 2, 0)
    ]
    preview_root.mkdir(parents=True, exist_ok=True)
    preview_frames[0].save(
        preview_root / f"chloe-r2-{variant}-idle.gif",
        save_all=True,
        append_images=preview_frames[1:],
        duration=(700, 140, 700, 100, 700),
        loop=0,
        disposal=2,
    )


def main(argv: list[str]) -> int:
    selected = tuple(argv) or VARIANTS
    unknown = set(selected) - set(VARIANTS)
    if unknown:
        raise SystemExit(f"unknown Chloe variant(s): {', '.join(sorted(unknown))}")
    for candidate in selected:
        save_variant(candidate)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
