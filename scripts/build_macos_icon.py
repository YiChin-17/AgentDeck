"""Compose a macOS-compliant app icon from a bare artwork source.

macOS Big Sur+ no longer auto-masks Dock icons; apps must ship the squircle
themselves. This script wraps the source artwork in Apple's standard
continuous-corner squircle (G2 superellipse, n=5) on a 1024x1024 canvas with
an 824x824 icon body, matching Apple's macOS app icon production template.

Usage:
    python scripts/build_macos_icon.py
    # then regenerate all bundle assets:
    npx tauri icon src-tauri/icons/icon.png

Inputs:
    src-tauri/icons/icon-source.png   bare artwork on a transparent background
                                      (full bleed, square aspect preferred)

Output:
    src-tauri/icons/icon.png          1024x1024 squircle-wrapped icon

Tunables:
    INNER_FRACTION  how much of the squircle the artwork fills (0..1)
    SQUIRCLE_N      superellipse exponent; ~5 matches Apple's continuous corner
    SSAA            mask supersampling factor for anti-aliased edges
"""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

REPO = Path(__file__).resolve().parent.parent
ICON_DIR = REPO / "src-tauri" / "icons"
SOURCE = ICON_DIR / "icon-source.png"
OUTPUT = ICON_DIR / "icon.png"

CANVAS = 1024
BODY = 824
SQUIRCLE_N = 5.0
INNER_FRACTION = 0.86
SSAA = 8


def make_squircle_mask(size: int, n: float) -> Image.Image:
    """Draw a supersampled superellipse mask and downscale it for AA."""
    big = size * SSAA
    half = big / 2.0
    points = []
    for index in range(2048):
        angle = math.tau * index / 2048
        cosine = math.cos(angle)
        sine = math.sin(angle)
        x = math.copysign(abs(cosine) ** (2.0 / n), cosine)
        y = math.copysign(abs(sine) ** (2.0 / n), sine)
        points.append((half + x * half, half + y * half))
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).polygon(points, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def compose_squircle_icon(
    source_path: Path,
    *,
    canvas_size: int = CANVAS,
    body_size: int = BODY,
    squircle_n: float = SQUIRCLE_N,
    inner_fraction: float = INNER_FRACTION,
) -> Image.Image:
    """Wrap bare artwork in the white squircle body used by every AgentDeck icon.

    Shared by the master icon (icon-source.png -> icon.png) and the small-size
    icon variant (icon-source-small.png), so both go through identical squircle
    geometry and only differ in their source artwork and raster dimensions.
    """
    if not source_path.exists():
        raise SystemExit(f"missing source artwork: {source_path}")

    src = Image.open(source_path).convert("RGBA")
    bbox = src.getbbox()
    if bbox:
        src = src.crop(bbox)

    canvas = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    sq_mask = make_squircle_mask(body_size, squircle_n)

    # Soft drop shadow for Dock depth.
    shadow_pad = 60
    shadow_layer = Image.new("RGBA", (body_size + shadow_pad * 2, body_size + shadow_pad * 2), (0, 0, 0, 0))
    shadow_alpha = Image.new("L", shadow_layer.size, 0)
    shadow_alpha.paste(sq_mask, (shadow_pad, shadow_pad))
    shadow_alpha = shadow_alpha.filter(ImageFilter.GaussianBlur(radius=14))
    shadow_alpha = Image.eval(shadow_alpha, lambda v: int(v * 60 / 255))
    shadow_rgba = Image.new("RGBA", shadow_layer.size, (0, 0, 0, 255))
    shadow_rgba.putalpha(shadow_alpha)
    sx = (canvas_size - shadow_layer.size[0]) // 2
    sy = (canvas_size - shadow_layer.size[1]) // 2 + 6
    canvas.alpha_composite(shadow_rgba, (sx, sy))

    # White squircle body.
    body = Image.new("RGBA", (body_size, body_size), (255, 255, 255, 255))
    body.putalpha(sq_mask)
    qx = (canvas_size - body_size) // 2
    qy = (canvas_size - body_size) // 2
    canvas.alpha_composite(body, (qx, qy))

    # Artwork inside the squircle, clipped to the mask.
    target = int(body_size * inner_fraction)
    sw, sh = src.size
    scale = min(target / sw, target / sh)
    new_size = (max(1, int(sw * scale)), max(1, int(sh * scale)))
    art = src.resize(new_size, Image.LANCZOS)

    art_layer = Image.new("RGBA", (body_size, body_size), (0, 0, 0, 0))
    cx = (body_size - new_size[0]) // 2
    cy = (body_size - new_size[1]) // 2
    art_layer.alpha_composite(art, (cx, cy))

    art_layer.putalpha(ImageChops.darker(art_layer.getchannel("A"), sq_mask))
    canvas.alpha_composite(art_layer, (qx, qy))

    return canvas


def main() -> None:
    canvas = compose_squircle_icon(SOURCE)
    canvas.save(OUTPUT, format="PNG", optimize=True)
    print(f"wrote {OUTPUT}  size={canvas.size}")
    print("next: npx tauri icon src-tauri/icons/icon.png")


if __name__ == "__main__":
    main()
