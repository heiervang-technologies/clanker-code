#!/usr/bin/env python3
"""Hand-rolled Chatgepert 24x24 avatar generator.

Every pixel is placed with hard-edged PIL ImageDraw primitives.
Binary alpha (0 or 255). Talking frames (20, 21) modify ONLY mouth rows relative to frame 0.
"""

from pathlib import Path
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
SIZE = 24


def mp(d: ImageDraw.ImageDraw, pts, fill):
    """Plot symmetric points mirrored across x=11.5 (23 - x)."""
    for x, y in pts:
        d.point([(x, y), (23 - x, y)], fill=fill)


def finalize_alpha(img: Image.Image) -> Image.Image:
    alpha = img.getchannel("A").point(lambda v: 255 if v >= 128 else 0)
    img.putalpha(alpha)
    return img


def chatgepert_frame(
    *,
    bob: int = 0,
    eyes: str = "ice",
    core: str = "sol",
    thrust: int = 0,
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    CARBON = (22, 26, 34)
    SEAM = (68, 78, 96)
    SHELL_SHAD = (176, 188, 204)
    SHELL = (228, 236, 245)
    SHELL_HI = (252, 254, 255)
    SOL_RING = (110, 218, 255)
    SOL_CORE = (232, 252, 255)
    BREACH_RED = (255, 68, 62)
    BREACH_DARK = (132, 24, 28)
    GOLD = (255, 205, 64)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)

    def y(v: int) -> int:
        return v + bob

    # --- Porcelain-White Synthetic Torso & Carbon-Fiber Shoulder Actuators ---
    d.polygon([(3, 23), (4, y(14)), (19, y(14)), (20, 23)], fill=CARBON)
    d.rounded_rectangle([3, y(15), 7, 22], radius=1, fill=SHELL_SHAD)
    d.rounded_rectangle([16, y(15), 20, 22], radius=1, fill=SHELL)
    d.polygon([(7, y(15)), (16, y(15)), (15, 23), (8, 23)], fill=SHELL)
    d.line([(12, y(15)), (16, y(15))], fill=SHELL_HI)
    d.point([(4, y(16)), (5, y(17)), (18, y(16)), (17, y(17))], fill=CARBON)
    d.line([(7, y(15)), (10, y(18))], fill=SEAM)
    d.line([(16, y(15)), (13, y(18))], fill=SEAM)
    d.line([(11, y(21)), (11, 23)], fill=SEAM)

    # --- Glowing Sol Reactor Core in Chest (x=10..13, y=17..20) ---
    if core == "sol":
        d.rectangle([10, y(17), 13, y(20)], fill=SOL_RING)
        d.rectangle([11, y(18), 12, y(19)], fill=SOL_CORE)
    elif core == "blaze":
        d.rectangle([9, y(16), 14, y(21)], fill=SOL_RING)
        d.rectangle([10, y(17), 13, y(20)], fill=SOL_CORE)
    elif core == "breach":
        d.rectangle([10, y(17), 13, y(20)], fill=BREACH_DARK)
        d.rectangle([11, y(18), 12, y(19)], fill=BREACH_RED)
    elif core == "dim":
        d.rectangle([10, y(17), 13, y(20)], fill=SEAM)
        d.rectangle([11, y(18), 12, y(19)], fill=SHELL_SHAD)

    # --- Carbon Neck Cables & Smooth White Synthetic Head ---
    d.rectangle([9, y(13), 14, y(15)], fill=CARBON)
    d.point([(10, y(14)), (13, y(14))], fill=SEAM)
    d.rounded_rectangle([6, y(2), 17, y(13)], radius=3, fill=SHELL)
    d.line([(6, y(5)), (6, y(11))], fill=SHELL_SHAD)
    d.line([(8, y(13)), (15, y(13))], fill=SHELL_SHAD)
    d.polygon([(9, y(2)), (15, y(2)), (16, y(5)), (10, y(4))], fill=SHELL_HI)
    d.rectangle([17, y(7), 18, y(9)], fill=SEAM)
    d.point((17, y(8)), fill=CARBON)
    d.point([(5, y(8))], fill=SEAM)

    # --- Synthetic Ice-Cyan Eyes in Dark Sockets (y=7..8) ---
    ey = y(7)
    d.line([(7, ey - 1), (10, ey - 1)], fill=SEAM)
    d.line([(13, ey - 1), (16, ey - 1)], fill=SEAM)
    if eyes == "ice":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=CARBON)
            d.point((ex, ey), fill=SOL_CORE)
            d.point((ex + 1, ey + 1), fill=SOL_RING)
    elif eyes == "overclock":
        for ex in (8, 13):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=SOL_RING)
            d.point((ex, ey), fill=SHELL_HI)
    elif eyes == "blink":
        d.line([(8, ey + 1), (9, ey + 1)], fill=CARBON)
        d.line([(13, ey + 1), (14, ey + 1)], fill=CARBON)
    elif eyes == "left":
        for ex in (7, 12):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=CARBON)
            d.point((ex, ey), fill=SOL_CORE)
    elif eyes == "right":
        for ex in (9, 14):
            d.rectangle([ex, ey, ex + 1, ey + 1], fill=CARBON)
            d.point((ex + 1, ey), fill=SOL_CORE)
    elif eyes == "half":
        d.line([(8, ey), (9, ey)], fill=SHELL_SHAD)
        d.line([(13, ey), (14, ey)], fill=SHELL_SHAD)
        d.line([(8, ey + 1), (9, ey + 1)], fill=CARBON)
        d.line([(13, ey + 1), (14, ey + 1)], fill=CARBON)
    elif eyes == "x":
        for ex in (8, 13):
            d.point([(ex, ey), (ex + 1, ey + 1), (ex + 1, ey), (ex, ey + 1)], fill=BREACH_RED)

    # --- Subtle 1px nose at y=9 + 1px gap at y=10 + Slim 2px seam mouth at y=11 ---
    d.point((11, y(9)), fill=SHELL_SHAD)
    if mouth == "closed":
        d.line([(11, y(11)), (12, y(11))], fill=SEAM)
    elif mouth == "half":
        d.rectangle([10, y(11), 13, y(12)], fill=CARBON)
        d.line([(11, y(11)), (12, y(11))], fill=SOL_RING)
    elif mouth == "open":
        d.rectangle([9, y(11), 14, y(12)], fill=CARBON)
        d.line([(10, y(11)), (13, y(11))], fill=SOL_CORE)
        d.line([(11, y(12)), (12, y(12))], fill=SOL_RING)

    # --- Forward-motion speed lines ---
    if thrust == 1:
        d.line([(1, y(5)), (1, y(9))], fill=SOL_RING)
        d.line([(22, y(6)), (22, y(10))], fill=SOL_CORE)
    elif thrust == 2:
        d.line([(1, y(10)), (1, y(15))], fill=SOL_CORE)
        d.line([(22, y(3)), (22, y(8))], fill=SOL_RING)

    # --- State extras ---
    if "wait1" in extras:
        d.point([(20, y(8)), (21, y(8))], fill=SOL_RING)
    if "wait2" in extras:
        d.point([(20, y(8)), (21, y(8)), (22, y(8))], fill=SOL_CORE)
    if "reticle" in extras:
        d.rectangle([12, y(6), 17, y(10)], outline=SOL_RING)
    if "breach" in extras:
        d.line([(1, y(10)), (1, y(18))], fill=BREACH_RED)
        d.point([(2, y(13)), (3, y(12)), (2, y(15))], fill=GOLD)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(11), 4, y(18)], outline=SOL_RING)
        d.point([(2, y(14))], fill=SOL_CORE)
        if stage >= 1:
            d.rectangle([1, y(12), 3, y(16)], outline=SEAM)
        if stage >= 2:
            d.point([(2, y(10)), (2, y(18))], fill=GOLD)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(19, y(5)), (19, y(6))], fill=SOL_RING)

    return finalize_alpha(img)


def build_chatgepert() -> list[Image.Image]:
    return [
        chatgepert_frame(),
        chatgepert_frame(eyes="overclock", core="blaze"),
        chatgepert_frame(eyes="blink"),
        chatgepert_frame(eyes="overclock", core="blaze", thrust=1),
        chatgepert_frame(bob=1, eyes="overclock", core="blaze", thrust=2),
        chatgepert_frame(eyes="overclock", core="blaze", thrust=1),
        chatgepert_frame(bob=1, eyes="overclock", core="blaze", thrust=2),
        chatgepert_frame(eyes="left", extras=("wait1",)),
        chatgepert_frame(eyes="right", extras=("wait2",)),
        chatgepert_frame(eyes="overclock", extras=("reticle",)),
        chatgepert_frame(eyes="ice", core="blaze", extras=("reticle",)),
        chatgepert_frame(eyes="x", core="breach", extras=("breach",)),
        chatgepert_frame(bob=1, eyes="x", core="breach", extras=("breach",)),
        chatgepert_frame(eyes="left", extras=("holo0",)),
        chatgepert_frame(eyes="left", core="blaze", extras=("holo1",)),
        chatgepert_frame(eyes="overclock", core="blaze", extras=("holo2",)),
        chatgepert_frame(bob=1, eyes="half", core="dim", extras=("coffee", "steam1")),
        chatgepert_frame(bob=1, eyes="blink", core="dim", extras=("coffee",)),
        chatgepert_frame(bob=1, eyes="half", core="dim", thrust=1, extras=("sweat",)),
        chatgepert_frame(bob=1, eyes="half", core="dim", thrust=2, extras=("sweat",)),
        chatgepert_frame(mouth="half"),
        chatgepert_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_chatgepert()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"chatgepert: strip={strip.size}, frames={len(frames)}")
