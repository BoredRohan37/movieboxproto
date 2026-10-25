#!/usr/bin/env python3
"""Generate the MovieBox app icon: SVG + PNG sizes + ICO bundle.

Design: "Midnight Cinema M" — a deep-space gradient rounded square, an ivory
M monogram whose valley is anchored by a gold play beam. Premium, crisp at
16px favicon and 512px store size alike.
"""
from PIL import Image, ImageDraw, ImageFilter

SIZE = 1024  # master canvas, downscaled from here

# --- palette -------------------------------------------------------------
NAVY_TOP = (13, 17, 43)
NAVY_MID = (24, 30, 74)
VIOLET = (45, 27, 88)
IVORY = (244, 246, 251)
GOLD_A = (255, 209, 102)
GOLD_B = (232, 145, 45)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def make_master():
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Background: vertical gradient navy -> violet inside a rounded square.
    pad = 40
    box = (pad, pad, SIZE - pad, SIZE - pad)
    radius = 240
    grad = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    top, bot = NAVY_TOP, VIOLET
    for y in range(pad, SIZE - pad):
        t = (y - pad) / (SIZE - 2 * pad)
        c = lerp(lerp(top, NAVY_MID, min(t * 1.6, 1.0)), bot, max(t - 0.55, 0) / 0.45)
        gd.line([(pad, y), (SIZE - pad, y)], fill=c + (255,))
    mask = Image.new("L", (SIZE, SIZE), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle(box, radius=radius, fill=255)
    img.paste(grad, (0, 0), mask)

    # Subtle top-left radial glow.
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    gld = ImageDraw.Draw(glow)
    gld.ellipse(
        (-SIZE * 0.25, -SIZE * 0.3, SIZE * 0.75, SIZE * 0.6),
        fill=(120, 140, 255, 60),
    )
    glow = glow.filter(ImageFilter.GaussianBlur(120))
    img = Image.alpha_composite(img, glow)
    img.putalpha(mask)

    d = ImageDraw.Draw(img)

    # Ivory M monogram (polyline with rounded joints).
    m_pts = [(285, 745), (285, 330), (512, 590), (739, 330), (739, 745)]
    d.line(m_pts, fill=IVORY + (255,), width=104, joint="curve")
    for p in (m_pts[0], m_pts[-1]):
        d.ellipse(
            (p[0] - 52, p[1] - 52, p[0] + 52, p[1] + 52), fill=IVORY + (255,)
        )

    # Gold play beam anchored in the M valley.
    tri = [(430, 425), (430, 645), (628, 535)]
    # soft shadow
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.polygon(tri, fill=(0, 0, 0, 140))
    shadow = shadow.filter(ImageFilter.GaussianBlur(18))
    img = Image.alpha_composite(img, shadow)

    tri_img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    td = ImageDraw.Draw(tri_img)
    for y in range(425, 646):
        t = (y - 425) / 220
        c = lerp(GOLD_A, GOLD_B, t)
        td.line([(430, y), (628, y)], fill=c + (255,))
    tri_mask = Image.new("L", (SIZE, SIZE), 0)
    tmd = ImageDraw.Draw(tri_mask)
    tmd.polygon(tri, fill=255)
    img.paste(tri_img, (0, 0), tri_mask)

    # Rounded gold ring accent around the play beam for depth.
    d.arc((390, 385, 670, 685), start=-58, end=58, fill=GOLD_A + (200,), width=10)

    # Zero RGB under fully-transparent pixels so opaque-compositing renderers
    # never show a halo outside the rounded square.
    px = img.load()
    for y in range(SIZE):
        for x in range(SIZE):
            r, g, b, a = px[x, y]
            if a == 0:
                px[x, y] = (0, 0, 0, 0)

    return img


def main():
    import os

    out_dir = "/home/z/my-project/MovieBox-GUI/gui"
    os.makedirs(out_dir, exist_ok=True)

    master = make_master()
    master.save(os.path.join(out_dir, "icon-master.png"))

    for size in (192, 512):
        master.resize((size, size), Image.LANCZOS).save(
            os.path.join(out_dir, f"icon-{size}.png")
        )

    # ICO bundle with the classic Windows sizes.
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    master.save(
        os.path.join(out_dir, "moviebox.ico"),
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
    )
    print("icon assets written to", out_dir)


if __name__ == "__main__":
    main()
