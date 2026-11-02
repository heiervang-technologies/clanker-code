#!/usr/bin/env python3
"""Hand-rolled Centurion avatar derived from the host fastfetch ANSI portrait.

The source portrait is a 60x30-cell true-color half-block illustration at
~/.config/fastfetch/hai-logo.ans.  Its identifying anchors survive the 24px
translation: low fedora, cyan visor, high trench collar, lit cigar, and a thin
smoke curl.  The frames are redrawn as pixel primitives rather than sampled so
every state remains readable through the ANSI half-block renderer.

Frame layout:
  0-2   idle          visor pulse, ember pulse
  3-6   running       coat swing, bob, wind-cut smoke
  7-8   waiting       visor glance, impatient ash
  9-10  review        visor scan and target reticle
  11-12 failed        red fault visor, dead cigar
  13-15 planning      cyan hologram accrues detail
  16-17 tired-idle    slouch, dim visor, long ash
  18-19 tired-running slouched coat swing
  20-21 talking       jaw grille only; closed mouth reuses frame 0
"""

from pathlib import Path

from PIL import Image
from PIL import ImageDraw


HERE = Path(__file__).resolve().parent
SIZE = 24

# Palette sampled from the fastfetch ANSI source, then consolidated so the
# tiny derivative reads as intentional pixel art instead of a noisy downsample.
VOID = (0, 27, 48)
NAVY = (0, 42, 74)
BLUE = (0, 88, 126)
BLUE_HI = (50, 158, 182)
TEAL = (24, 153, 174)
CYAN = (0, 255, 255)
ICE = (174, 226, 224)
PALE = (140, 189, 191)
BROWN = (78, 36, 12)
EMBER = (255, 83, 0)
EMBER_HI = (255, 155, 16)
FAULT = (255, 62, 54)
FAULT_DARK = (116, 29, 34)
ASH = (102, 117, 122)


def frame(
    *,
    bob=0,
    visor="bright",
    coat="rest",
    smoke="curl",
    cigar="lit",
    mouth="closed",
    extras=(),
):
    image = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    def y(value):
        return value + bob

    # Trench coat and split tails.  The pale lapels are as important to the
    # silhouette as the hat: together they keep this from becoming "cyan bot".
    shoulder_left = 2 if coat == "left" else 3
    shoulder_right = 21 if coat == "right" else 20
    draw.polygon(
        [
            (shoulder_left, y(17)),
            (6, y(14)),
            (17, y(14)),
            (shoulder_right, y(17)),
            (19, y(22)),
            (5, y(22)),
        ],
        fill=NAVY,
    )
    draw.rectangle([6, y(17), 17, y(22)], fill=BLUE)
    if coat == "slump":
        draw.rectangle([7, y(19), 16, y(22)], fill=NAVY)
    draw.polygon([(5, y(15)), (10, y(16)), (11, y(21)), (7, y(18))], fill=ICE)
    draw.polygon([(18, y(15)), (13, y(16)), (12, y(21)), (16, y(18))], fill=PALE)
    draw.polygon([(10, y(16)), (13, y(16)), (12, y(22))], fill=VOID)
    draw.line([(12, y(18)), (12, y(22))], fill=TEAL)

    # Arms and coat tails give whole-pixel motion without destabilizing the
    # recognizable head silhouette.
    if coat == "left":
        draw.rectangle([2, y(17), 4, y(20)], fill=BLUE)
        draw.polygon([(7, y(21)), (9, y(23)), (11, y(21))], fill=NAVY)
        draw.polygon([(15, y(21)), (18, y(22)), (17, y(19))], fill=VOID)
    elif coat == "right":
        draw.rectangle([19, y(17), 21, y(20)], fill=BLUE)
        draw.polygon([(13, y(21)), (15, y(23)), (17, y(21))], fill=NAVY)
        draw.polygon([(6, y(21)), (8, y(22)), (7, y(19))], fill=VOID)
    else:
        draw.rectangle([3, y(17), 5, y(20)], fill=BLUE)
        draw.rectangle([18, y(17), 20, y(20)], fill=NAVY)

    # Head, fedora crown, and low brim.
    draw.rounded_rectangle([5, y(5), 18, y(15)], radius=2, fill=NAVY)
    draw.polygon([(5, y(5)), (6, y(2)), (9, y(0)), (16, y(1)), (18, y(4))], fill=BLUE)
    draw.polygon([(7, y(2)), (10, y(0)), (16, y(1)), (17, y(3)), (10, y(3))], fill=BLUE_HI)
    draw.polygon([(4, y(4)), (18, y(3)), (21, y(5)), (18, y(6)), (5, y(7)), (3, y(6))], fill=NAVY)
    draw.line([(6, y(4)), (18, y(4))], fill=TEAL)
    draw.point([(6, y(3)), (7, y(3)), (17, y(2))], fill=ICE)

    # Visor band.  Glance states shift only the hot core; scan adds a moving
    # pale bar; fault deliberately drops the source palette for an alarm read.
    draw.polygon([(5, y(7)), (18, y(6)), (19, y(10)), (17, y(12)), (6, y(12)), (4, y(10))], fill=BLUE)
    draw.rectangle([6, y(8), 17, y(10)], fill=TEAL)
    if visor == "bright":
        draw.rectangle([8, y(8), 16, y(9)], fill=CYAN)
        draw.line([(7, y(10)), (17, y(10))], fill=ICE)
    elif visor == "pulse":
        draw.rectangle([7, y(8), 17, y(10)], fill=CYAN)
        draw.line([(9, y(8)), (15, y(8))], fill=(214, 255, 255))
    elif visor == "blink":
        draw.line([(7, y(9)), (17, y(9))], fill=CYAN)
    elif visor == "left":
        draw.rectangle([6, y(8), 12, y(10)], fill=CYAN)
        draw.line([(13, y(9)), (17, y(9))], fill=BLUE_HI)
    elif visor == "right":
        draw.rectangle([12, y(8), 17, y(10)], fill=CYAN)
        draw.line([(7, y(9)), (11, y(9))], fill=BLUE_HI)
    elif visor == "scan-left":
        draw.rectangle([7, y(8), 17, y(10)], fill=TEAL)
        draw.rectangle([8, y(8), 9, y(10)], fill=CYAN)
    elif visor == "scan-right":
        draw.rectangle([7, y(8), 17, y(10)], fill=TEAL)
        draw.rectangle([15, y(8), 16, y(10)], fill=CYAN)
    elif visor == "fault":
        draw.rectangle([7, y(8), 17, y(10)], fill=FAULT_DARK)
        draw.line([(7, y(8)), (17, y(10))], fill=FAULT)
        draw.line([(7, y(10)), (17, y(8))], fill=FAULT)
    elif visor == "dim":
        draw.line([(7, y(10)), (17, y(10))], fill=BLUE_HI)

    # Jaw grille; talking frames change only this local region relative to
    # frame zero, satisfying the mouth-flap continuity contract.
    draw.rectangle([8, y(12), 16, y(14)], fill=VOID)
    if mouth == "closed":
        draw.line([(9, y(13)), (15, y(13))], fill=BLUE_HI)
    elif mouth == "half":
        draw.rectangle([10, y(13), 15, y(13)], fill=ICE)
    elif mouth == "open":
        draw.rectangle([10, y(12), 15, y(14)], fill=VOID)
        draw.line([(10, y(12)), (15, y(12))], fill=ICE)
        draw.line([(11, y(14)), (14, y(14))], fill=TEAL)

    # Cigar, ember, and smoke curl from the original portrait.
    cigar_y = y(12)
    if cigar != "gone":
        draw.rectangle([16, cigar_y, 21, cigar_y + 1], fill=BROWN)
        if cigar == "lit":
            draw.rectangle([21, cigar_y - 1, 22, cigar_y + 1], fill=EMBER)
            draw.point([(22, cigar_y - 1)], fill=EMBER_HI)
        elif cigar == "ash":
            draw.rectangle([20, cigar_y, 22, cigar_y + 1], fill=ASH)
        elif cigar == "dead":
            draw.rectangle([21, cigar_y, 22, cigar_y + 1], fill=FAULT_DARK)

    if smoke == "curl":
        draw.point([(22, y(10)), (23, y(8)), (22, y(6)), (22, y(5)), (23, y(3))], fill=ICE)
        draw.point([(21, y(7)), (23, y(2))], fill=PALE)
    elif smoke == "puff":
        draw.point([(22, y(9)), (23, y(8)), (21, y(7)), (22, y(6)), (23, y(6))], fill=ICE)
    elif smoke == "wind-left":
        draw.line([(21, y(10)), (16, y(8))], fill=PALE)
        draw.point([(14, y(8)), (18, y(9))], fill=ICE)
    elif smoke == "wind-right":
        draw.line([(22, y(9)), (19, y(7))], fill=ICE)
        draw.point([(17, y(7)), (20, y(8))], fill=PALE)
    elif smoke == "question":
        draw.line([(22, y(9)), (23, y(7)), (21, y(6)), (22, y(4))], fill=ICE)
        draw.point([(22, y(2))], fill=ICE)
    elif smoke == "fault":
        draw.point([(21, y(9)), (22, y(8)), (20, y(6)), (23, y(5))], fill=ASH)

    if "reticle" in extras:
        draw.rectangle([13, y(7), 18, y(11)], outline=EMBER_HI)
        draw.point([(15, y(9)), (16, y(9))], fill=EMBER)
    if "ash-fall" in extras:
        draw.point([(23, y(15)), (22, y(17))], fill=ASH)
    if "holo0" in extras or "holo1" in extras or "holo2" in extras:
        stage = int(next(extra[-1] for extra in extras if extra.startswith("holo")))
        draw.rectangle([0, y(13), 4, y(19)], outline=TEAL)
        draw.point([(1, y(15)), (3, y(15))], fill=CYAN)
        if stage >= 1:
            draw.line([(1, y(17)), (3, y(17))], fill=ICE)
        if stage >= 2:
            draw.point([(2, y(18)), (4, y(14))], fill=CYAN)
    if "sweat" in extras:
        draw.point([(20, y(6)), (21, y(7))], fill=CYAN)

    return image


FRAMES = [
    frame(),
    frame(visor="pulse", smoke="puff"),
    frame(visor="blink"),
    frame(coat="left", smoke="wind-left"),
    frame(bob=1, coat="right", smoke="wind-right"),
    frame(coat="right", smoke="wind-left"),
    frame(bob=1, coat="left", smoke="wind-right"),
    frame(visor="left", smoke="question", cigar="ash"),
    frame(visor="right", smoke="puff", extras=("ash-fall",)),
    frame(visor="scan-left", smoke="curl"),
    frame(visor="scan-right", smoke="curl", extras=("reticle",)),
    frame(visor="fault", smoke="fault", cigar="dead"),
    frame(bob=1, visor="fault", smoke="fault", cigar="gone", coat="slump"),
    frame(visor="scan-left", smoke="puff", extras=("holo0",)),
    frame(visor="scan-right", smoke="puff", extras=("holo1",)),
    frame(visor="bright", smoke="curl", extras=("holo2",)),
    frame(bob=1, visor="dim", coat="slump", cigar="ash", smoke="puff"),
    frame(bob=1, visor="blink", coat="slump", cigar="ash", smoke="curl"),
    frame(bob=1, visor="dim", coat="left", cigar="ash", smoke="wind-left"),
    frame(bob=1, visor="dim", coat="right", cigar="ash", smoke="wind-right", extras=("sweat",)),
    frame(mouth="half"),
    frame(mouth="open"),
]

strip = Image.new("RGBA", (SIZE * len(FRAMES), SIZE), (0, 0, 0, 0))
for index, source in enumerate(FRAMES):
    alpha = source.getchannel("A").point(lambda value: 255 if value >= 128 else 0)
    source.putalpha(alpha)
    strip.paste(source, (index * SIZE, 0))

strip.save(HERE / "sheet.png")
print(f"strip: {strip.size}, frames: {len(FRAMES)}")
