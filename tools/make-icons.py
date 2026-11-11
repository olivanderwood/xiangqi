#!/usr/bin/env python3
"""Generate the app icons from crates/xq-app/assets/icon.png (512x512).

Outputs:
  crates/xq-app/assets/icon.ico   -- Windows, sizes 16..256 px (PIL)
  crates/xq-app/assets/icon.icns  -- macOS, PNG elements + classic 128 px
                                     raw RGB/mask fallback

The .icns format (Apple Icon Image): file starts with b'icns' + a
big-endian total length, then elements of (4-byte type, 4-byte
big-endian length incl. the 8-byte header, data). Large elements
(ic07..ic10) are JPEG 2000 in icons made by Apple's iconutil; this
script has no JPEG 2000 encoder on Linux (Pillow here is built
without OpenJPEG), so it stores PNG data in icp4..ic10 instead --
the same fallback libicns/png2icns use, accepted by macOS Icon
Services -- plus the classic uncompressed 'it32'/'t8mk' pair at
128 px, which any decoder can read. Verify the icon visually on a
Mac before release (docs/release-checklist.md, item 1).

Requires: python3 + Pillow. Re-run after replacing icon.png.
"""

import struct
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "crates" / "xq-app" / "assets" / "icon.png"
ICO_OUT = ROOT / "crates" / "xq-app" / "assets" / "icon.ico"
ICNS_OUT = ROOT / "crates" / "xq-app" / "assets" / "icon.icns"

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
# (OSType, px) for PNG elements, smallest first.
ICNS_PNG = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic09", 512),
    (b"ic10", 1024),
]


def png_bytes(img: Image.Image, size: int) -> bytes:
    import io

    resized = img.resize((size, size), Image.LANCZOS)
    buf = io.BytesIO()
    resized.save(buf, format="PNG")
    return buf.getvalue()


def element(ostype: bytes, data: bytes) -> bytes:
    return ostype + struct.pack(">I", 8 + len(data)) + data


def make_ico(img: Image.Image) -> None:
    img.save(ICO_OUT, format="ICO", sizes=[(s, s) for s in ICO_SIZES])
    print(f"wrote {ICO_OUT} ({ICO_OUT.stat().st_size} bytes)")


def make_icns(img: Image.Image) -> None:
    elements = b""
    for ostype, size in ICNS_PNG:
        # The source is 512 px; 1024 is an upscale, kept so Retina
        # (512@2x) has an entry instead of nothing.
        elements += element(ostype, png_bytes(img, size))
    # Classic 128 px fallback: 24-bit RGB + 8-bit alpha mask.
    rgb128 = img.resize((128, 128), Image.LANCZOS).convert("RGB")
    alpha128 = img.resize((128, 128), Image.LANCZOS).convert("RGBA")
    elements += element(b"it32", rgb128.tobytes())
    elements += element(b"t8mk", alpha128.getchannel("A").tobytes())
    total = 8 + len(elements)
    ICNS_OUT.write_bytes(b"icns" + struct.pack(">I", total) + elements)
    print(f"wrote {ICNS_OUT} ({ICNS_OUT.stat().st_size} bytes)")


def validate_icns() -> None:
    data = ICNS_OUT.read_bytes()
    assert data[:4] == b"icns", "bad icns magic"
    (total,) = struct.unpack(">I", data[4:8])
    assert total == len(data), f"icns length {total} != file size {len(data)}"
    off = 8
    seen = []
    while off < len(data):
        ostype = data[off : off + 4]
        (length,) = struct.unpack(">I", data[off + 4 : off + 8])
        assert length >= 8 and off + length <= len(data), f"bad element {ostype}"
        payload = data[off + 8 : off + length]
        if ostype in (b"icp4", b"icp5", b"icp6", b"ic07", b"ic08", b"ic09", b"ic10"):
            assert payload[:8] == b"\x89PNG\r\n\x1a\n", f"{ostype} is not PNG"
        seen.append(ostype.decode("latin-1"))
        off += length
    assert off == len(data), "trailing bytes in icns"
    print(f"icns OK: elements {seen}")


def validate_ico() -> None:
    with Image.open(ICO_OUT) as im:
        sizes = set(im.info.get("sizes", []))
    missing = {s for s in ICO_SIZES if (s, s) not in sizes}
    assert not missing, f"ico missing sizes {missing}"
    print(f"ico OK: sizes {sorted(sizes)}")


def main() -> int:
    if not SRC.exists():
        print(f"missing source icon: {SRC}", file=sys.stderr)
        return 1
    with Image.open(SRC) as im:
        img = im.convert("RGBA")
        assert img.size == (512, 512), f"expected 512x512, got {img.size}"
        make_ico(img)
        make_icns(img)
    validate_ico()
    validate_icns()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
