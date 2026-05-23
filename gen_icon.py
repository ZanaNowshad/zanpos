"""
Generate ZAN POS "Z" icon for all required Tauri icon sizes.
Background: dark charcoal (#1a1a1a)
Letter: gold (#F0A500)  -- matches the app's accent color
"""
from PIL import Image, ImageDraw, ImageFont
import os, struct, zlib

ICONS_DIR = os.path.join(os.path.dirname(__file__), "src-tauri", "icons")

GOLD   = (240, 165, 0, 255)
BG     = (22, 22, 26, 255)   # --bg from design system

def draw_z(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), BG)
    d   = ImageDraw.Draw(img)

    # Rounded rectangle background (full square, slight corner radius)
    r = max(2, size // 8)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=r, fill=BG)

    # Draw "Z" manually with thick lines — looks crisp at any size
    pad  = size * 0.14
    lw   = max(2, int(size * 0.1))   # stroke width

    x0, y0 = pad,          pad
    x1, y1 = size - pad,   pad
    x2, y2 = pad,          size - pad
    x3, y3 = size - pad,   size - pad

    # Top bar
    d.line([x0, y0, x1, y1], fill=GOLD, width=lw)
    # Diagonal
    d.line([x1, y0, x2, y3], fill=GOLD, width=lw)
    # Bottom bar
    d.line([x2, y3, x3, y3], fill=GOLD, width=lw)

    return img

def save_png(img: Image.Image, path: str):
    img.save(path, "PNG")
    print(f"  wrote {path}")

def make_ico(sizes: list[int], path: str):
    """Build a multi-resolution .ico from scratch (no wand/imagemagick needed)."""
    frames = []
    for s in sizes:
        img = draw_z(s).convert("RGBA")
        import io
        buf = io.BytesIO()
        img.save(buf, "PNG")
        frames.append((s, buf.getvalue()))

    # ICO header
    n = len(frames)
    header = struct.pack("<HHH", 0, 1, n)  # reserved, type=1 (ICO), count
    # directory entries (16 bytes each) + image data
    dir_offset = 6 + 16 * n
    dir_entries = b""
    image_data  = b""
    for s, data in frames:
        w = h = s if s < 256 else 0   # 0 means 256 in ICO
        dir_entries += struct.pack("<BBBBHHII",
            w, h, 0, 0, 1, 32,
            len(data), dir_offset + len(image_data))
        image_data  += data

    with open(path, "wb") as f:
        f.write(header + dir_entries + image_data)
    print(f"  wrote {path}")

print("Generating ZAN POS 'Z' icons...")

# Standard PNG sizes Tauri expects
sizes_png = {
    "32x32.png":        32,
    "128x128.png":      128,
    "128x128@2x.png":   256,
    "icon.png":         512,
    # Windows Square logos
    "Square30x30Logo.png":   30,
    "Square44x44Logo.png":   44,
    "Square71x71Logo.png":   71,
    "Square89x89Logo.png":   89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png":         50,
}

for fname, sz in sizes_png.items():
    save_png(draw_z(sz), os.path.join(ICONS_DIR, fname))

# .ico (16, 32, 48, 256)
make_ico([16, 32, 48, 256], os.path.join(ICONS_DIR, "icon.ico"))

# .icns — build with iconutil on macOS; on Windows just copy the 512 PNG
icns_path = os.path.join(ICONS_DIR, "icon.icns")
try:
    import subprocess, io, tempfile, shutil
    # Try macOS iconutil approach (no-op on Windows)
    tmp = tempfile.mkdtemp(suffix=".iconset")
    mapping = {16:"16x16",32:"32x32",64:"64x64",128:"128x128",256:"256x256",512:"512x512"}
    for s, name in mapping.items():
        draw_z(s).save(os.path.join(tmp, f"icon_{name}.png"))
        draw_z(s*2).save(os.path.join(tmp, f"icon_{name}@2x.png"))
    result = subprocess.run(["iconutil", "-c", "icns", tmp, "-o", icns_path],
                            capture_output=True)
    if result.returncode == 0:
        print(f"  wrote {icns_path}")
    else:
        raise RuntimeError("iconutil not available")
except Exception:
    # Windows fallback: copy the 512 PNG as a placeholder
    shutil.copy(os.path.join(ICONS_DIR, "icon.png"), icns_path)
    print(f"  wrote {icns_path} (PNG placeholder — rebuild on macOS for real .icns)")

print("Done.")
