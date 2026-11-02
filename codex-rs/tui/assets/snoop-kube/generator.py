#!/usr/bin/env python3
"""Hand-rolled Snoop-Kube 24x24 avatar generator.

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


def snoop_kube_frame(
    *,
    bob: int = 0,
    shades: str = "glint",
    braids: int = 0,
    salute: bool = False,
    mouth: str = "closed",
    extras: tuple[str, ...] = (),
) -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    INK = (16, 18, 24)
    CAP_NAVY = (28, 42, 82)
    CAP_BRIM = (18, 28, 58)
    K8S_BLUE = (46, 132, 242)
    K8S_CYAN = (115, 230, 255)
    WHITE = (245, 248, 255)
    SKIN = (166, 114, 74)
    SKIN_SHAD = (132, 86, 52)
    HAIR = (24, 20, 18)
    BEAD = (238, 240, 245)
    SHADES = (14, 16, 22)
    SHADES_RIM = (52, 58, 72)
    SHIRT = (32, 48, 92)
    SHIRT_DARK = (22, 34, 68)
    ZIP = (176, 184, 198)
    RED = (240, 68, 62)
    COFFEE = (132, 84, 46)
    STEAM = (210, 215, 225)

    def y(v: int) -> int:
        return v + bob

    # --- Navy collared work shirt with center zipper ---
    d.polygon([(3, 23), (5, y(16)), (18, y(16)), (20, 23)], fill=SHIRT)
    d.rectangle([5, y(17), 18, 23], fill=SHIRT)
    mp(d, [(8, y(16)), (9, y(16)), (9, y(17)), (10, y(17))], SHIRT_DARK)
    d.line([(11, y(17)), (11, 23)], fill=ZIP)
    d.line([(12, y(17)), (12, 23)], fill=SHIRT_DARK)
    d.line([(14, y(19)), (16, y(19))], fill=K8S_CYAN)

    # --- Twin side braids (roots anchored at x=5,18; lower tips swing with `braids`) ---
    for root_x, is_left in ((5, True), (18, False)):
        d.line([(root_x, y(10)), (root_x, y(14))], fill=HAIR)
        d.point([(root_x, y(12))], fill=INK)
        tip_x = root_x + braids
        d.line([(tip_x, y(14)), (tip_x, y(16))], fill=HAIR)
        # White cuff beads at bottom tips
        d.point([(tip_x, y(16)), (tip_x, y(17))], fill=BEAD)
        d.point([(tip_x - 1 if is_left else tip_x + 1, y(17))], fill=BEAD)

    # --- Head & Ears ---
    mp(d, [(6, y(9)), (6, y(10))], SKIN_SHAD)
    d.rounded_rectangle([7, y(5), 16, y(15)], radius=2, fill=SKIN)
    d.line([(8, y(15)), (15, y(15))], fill=SKIN_SHAD)

    # --- Wraparound Dark Sunglasses (fused with brow at y=7..9) ---
    d.rectangle([6, y(7), 17, y(9)], fill=SHADES_RIM)
    mp(d, [(7, y(8)), (8, y(8)), (9, y(8)), (10, y(8)), (7, y(9)), (8, y(9)), (9, y(9)), (10, y(9))], SHADES)
    if shades == "glint":
        mp(d, [(8, y(8)), (9, y(8))], WHITE)
    elif shades == "pulse":
        mp(d, [(8, y(8)), (9, y(8))], K8S_CYAN)
        mp(d, [(7, y(8))], WHITE)
    elif shades == "blink":
        mp(d, [(8, y(9)), (9, y(9))], SHADES_RIM)
    elif shades == "left":
        for lx in (7, 13):
            d.point([(lx, y(8)), (lx + 1, y(8))], fill=WHITE)
    elif shades == "right":
        for lx in (9, 15):
            d.point([(lx, y(8)), (lx + 1, y(8))], fill=WHITE)
    elif shades == "scan":
        mp(d, [(8, y(8)), (9, y(8)), (8, y(9)), (9, y(9))], K8S_CYAN)
    elif shades == "fault":
        for lx in (8, 14):
            d.point([(lx, y(8)), (lx + 1, y(9)), (lx + 1, y(8)), (lx, y(9))], fill=RED)
    elif shades == "dim":
        mp(d, [(8, y(9))], SHADES_RIM)

    # --- Nose (y=10) + 1px skin gap at y=11 ---
    mp(d, [(11, y(10))], SKIN_SHAD)

    # --- Slim Goatee & Drooping Mustache Tips (y=12..15) ---
    mp(d, [(10, y(12)), (11, y(12))], HAIR)
    mp(d, [(9, y(13)), (9, y(14))], HAIR)
    mp(d, [(10, y(15)), (11, y(15))], HAIR)

    # Mouth inside goatee (y=13..14, x=10..13) — talking modifies ONLY this region!
    if mouth == "closed":
        mp(d, [(10, y(13)), (11, y(13))], SKIN_SHAD)
        mp(d, [(10, y(14)), (11, y(14))], SKIN)
    elif mouth == "half":
        mp(d, [(10, y(13)), (11, y(13))], INK)
        mp(d, [(11, y(13))], WHITE)
        mp(d, [(10, y(14)), (11, y(14))], SKIN_SHAD)
    elif mouth == "open":
        mp(d, [(10, y(13)), (11, y(13)), (10, y(14)), (11, y(14))], INK)
        mp(d, [(10, y(13)), (11, y(13))], WHITE)
        mp(d, [(11, y(14))], (210, 95, 95))

    # --- Navy K8s Cap (y=1..6) with spoked helm wheel ---
    d.rounded_rectangle([6, y(1), 17, y(5)], radius=2, fill=CAP_NAVY)
    d.rectangle([5, y(5), 18, y(6)], fill=CAP_BRIM)
    # Kubernetes helm wheel emblem (4x3 with spoke corners and dark hub)
    d.rectangle([10, y(1), 13, y(4)], fill=K8S_BLUE)
    mp(d, [(10, y(1)), (11, y(1)), (10, y(2)), (10, y(3)), (11, y(4))], K8S_CYAN)
    mp(d, [(11, y(2)), (11, y(3))], CAP_NAVY)  # inner hub ring
    mp(d, [(11, y(2))], WHITE)                 # helm wheel center star

    # --- Optional Crisp Salute Arm ---
    if salute:
        d.polygon([(1, y(11)), (4, y(6)), (6, y(7)), (4, y(13))], fill=SKIN)
        d.rectangle([1, y(13), 4, y(17)], fill=SHIRT)

    # --- State extras ---
    if "wait1" in extras:
        d.point([(20, y(8)), (21, y(8))], fill=K8S_CYAN)
    if "wait2" in extras:
        d.point([(20, y(8)), (21, y(8)), (22, y(8))], fill=WHITE)
    if "reticle" in extras:
        d.rectangle([13, y(6), 18, y(10)], outline=K8S_CYAN)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(e[-1] for e in extras if e.startswith("holo")))
        d.rectangle([0, y(12), 4, y(18)], outline=K8S_BLUE)
        d.point([(2, y(14))], fill=K8S_CYAN)
        if stage >= 1:
            d.point([(1, y(16)), (3, y(16))], fill=WHITE)
        if stage >= 2:
            d.line([(1, y(15)), (3, y(15))], fill=K8S_CYAN)
    if "coffee" in extras:
        d.rectangle([19, 19, 22, 22], fill=COFFEE)
        d.line([(19, 19), (22, 19)], fill=STEAM)
        if "steam1" in extras:
            d.point([(20, 17), (21, 15)], fill=STEAM)
        else:
            d.point([(21, 17), (20, 15)], fill=STEAM)
    if "sweat" in extras:
        d.point([(20, y(6)), (20, y(7))], fill=K8S_CYAN)

    return finalize_alpha(img)


def build_snoop_kube() -> list[Image.Image]:
    return [
        snoop_kube_frame(),
        snoop_kube_frame(shades="pulse"),
        snoop_kube_frame(shades="blink"),
        snoop_kube_frame(braids=-1, salute=True, shades="pulse"),
        snoop_kube_frame(bob=1, braids=0, salute=True),
        snoop_kube_frame(braids=1, salute=True, shades="pulse"),
        snoop_kube_frame(bob=1, braids=0, salute=True),
        snoop_kube_frame(shades="left", extras=("wait1",)),
        snoop_kube_frame(shades="right", extras=("wait2",)),
        snoop_kube_frame(shades="scan"),
        snoop_kube_frame(shades="scan", extras=("reticle",)),
        snoop_kube_frame(shades="fault"),
        snoop_kube_frame(bob=1, shades="fault"),
        snoop_kube_frame(shades="left", extras=("holo0",)),
        snoop_kube_frame(shades="pulse", extras=("holo1",)),
        snoop_kube_frame(shades="glint", extras=("holo2",)),
        snoop_kube_frame(bob=1, shades="dim", extras=("coffee", "steam1")),
        snoop_kube_frame(bob=1, shades="blink", extras=("coffee",)),
        snoop_kube_frame(bob=1, shades="dim", braids=-1, extras=("sweat",)),
        snoop_kube_frame(bob=1, shades="dim", braids=1, extras=("sweat",)),
        snoop_kube_frame(mouth="half"),
        snoop_kube_frame(mouth="open"),
    ]

if __name__ == "__main__":
    frames = build_snoop_kube()
    strip = Image.new("RGBA", (SIZE * len(frames), SIZE), (0, 0, 0, 0))
    for index, source in enumerate(frames):
        strip.paste(source, (index * SIZE, 0))
    strip.save(HERE / "sheet.png")
    print(f"snoop-kube: strip={strip.size}, frames={len(frames)}")
