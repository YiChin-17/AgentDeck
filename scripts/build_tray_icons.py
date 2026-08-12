"""Generate tray icons for macOS menu bar and Windows/Linux notification area.

macOS variant (tray-icon-{N}.png): the AgentDeck card-deck silhouette stamped
as solid black on a transparent background. Tauri marks it as a template image
so macOS supplies the correct light/dark menu-bar tint.

Windows/Linux variant (tray-icon-color-{N}.png): the full colored app icon
resampled to tray sizes. Windows taskbars don't auto-tint, and a single-tone
silhouette is invisible against either light or dark taskbars, so we use the
branded artwork which has its own contrast in both themes.

Inputs:
    src-tauri/icons/icon-source.png   bare artwork on transparent background
    src-tauri/icons/icon.png          full colored app icon (rounded square)

Outputs:
    src-tauri/icons/tray/tray-icon-source.png                (macOS master)
    src-tauri/icons/tray/tray-icon-{16,20,24,32}.png         (macOS)
    src-tauri/icons/tray/tray-icon-color-{16,20,24,32}.png   (Windows/Linux)
"""
from __future__ import annotations

from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parent.parent
SOURCE = REPO / "src-tauri" / "icons" / "icon-source.png"
COLOR_SOURCE = REPO / "src-tauri" / "icons" / "icon.png"
TRAY_DIR = REPO / "src-tauri" / "icons" / "tray"
TRAY_SOURCE = TRAY_DIR / "tray-icon-source.png"
SIZES = (16, 20, 24, 32)
INSET = 0.0         # fraction of canvas left blank around the glyph
GLYPH_RGB = (0, 0, 0)  # template pixels; macOS supplies the visible tint
SSAA = 4            # render oversized then downscale for clean small sizes
ALPHA_THRESHOLD = 8 # alpha levels below this become fully transparent


def silhouette(src: Image.Image) -> Image.Image:
    """Convert source artwork to a solid-black RGBA silhouette."""
    bbox = src.getbbox()
    if bbox:
        src = src.crop(bbox)
    alpha = src.getchannel("A").point(lambda value: 0 if value < ALPHA_THRESHOLD else value)
    rgba = Image.new("RGBA", src.size, (*GLYPH_RGB, 0))
    rgba.putalpha(alpha)
    return rgba


def render_at(silh: Image.Image, size: int) -> Image.Image:
    big = size * SSAA
    canvas = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    target = int(big * (1.0 - 2 * INSET))
    sw, sh = silh.size
    scale = min(target / sw, target / sh)
    new_size = (max(1, int(sw * scale)), max(1, int(sh * scale)))
    scaled = silh.resize(new_size, Image.LANCZOS)
    cx = (big - new_size[0]) // 2
    cy = (big - new_size[1]) // 2
    canvas.alpha_composite(scaled, (cx, cy))
    return canvas.resize((size, size), Image.LANCZOS)


def render_color(src: Image.Image, size: int) -> Image.Image:
    """Downscale the full-color app icon to a tray size with SSAA."""
    big = size * SSAA
    scaled = src.resize((big, big), Image.LANCZOS)
    return scaled.resize((size, size), Image.LANCZOS)


def main() -> None:
    if not SOURCE.exists():
        raise SystemExit(f"missing source: {SOURCE}")
    if not COLOR_SOURCE.exists():
        raise SystemExit(f"missing color source: {COLOR_SOURCE}")
    src = Image.open(SOURCE).convert("RGBA")
    color_src = Image.open(COLOR_SOURCE).convert("RGBA")
    silh = silhouette(src)
    TRAY_DIR.mkdir(parents=True, exist_ok=True)
    render_at(silh, 512).save(TRAY_SOURCE, format="PNG", optimize=True)
    print(f"wrote {TRAY_SOURCE}  size=512")
    for size in SIZES:
        out = TRAY_DIR / f"tray-icon-{size}.png"
        render_at(silh, size).save(out, format="PNG", optimize=True)
        print(f"wrote {out}  size={size}")
        color_out = TRAY_DIR / f"tray-icon-color-{size}.png"
        render_color(color_src, size).save(color_out, format="PNG", optimize=True)
        print(f"wrote {color_out}  size={size}")


if __name__ == "__main__":
    main()
