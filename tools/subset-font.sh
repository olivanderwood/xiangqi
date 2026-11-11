#!/usr/bin/env bash
# Subset fonts for xq-app (spec SS7.5 / T3.1).
#
# Generates:
#   tools/cjk_chars.txt                          -- char list (regenerated here)
#   crates/xq-app/assets/fonts/NotoSerifCJKSC-Bold-subset.ttf
#   crates/xq-app/assets/fonts/NotoSerifCJKSC-Regular-subset.ttf
#   crates/xq-app/assets/fonts/NotoSans-Regular-subset.ttf   (Vietnamese + Latin)
#   crates/xq-app/assets/fonts/NotoSans-Bold-subset.ttf
#
# The system .ttc files hold 5 faces (JP, KR, SC, TC, HK); the SC face is
# index 2. `pyftsubset` cannot read .ttc directly, so the SC face is first
# extracted to a temporary .ttf with fontTools, then subset.
# Re-run after editing crates/xq-book/data/openings.json: the char list is
# rebuilt from the book file, so new CJK chars are picked up automatically.
#
# Requires: python3 + fontTools (pyftsubset) and the Noto fonts installed
# on the system (Debian: fonts-noto-cjk, fonts-noto-core).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/crates/xq-app/assets/fonts"
mkdir -p "$OUT"

SERIF_BOLD_TTC="${SERIF_BOLD_TTC:-/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc}"
SERIF_REG_TTC="${SERIF_REG_TTC:-/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc}"
SANS_REG="${SANS_REG:-/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf}"
SANS_BOLD="${SANS_BOLD:-/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf}"

# 1. Regenerate the CJK char list: fixed set + every CJK char in the book.
python3 - "$ROOT" <<'PYEOF'
import json, sys
from pathlib import Path
root = Path(sys.argv[1])
fixed = (
    # 14 piece chars (traditional, fixed set SS7.2)
    "帥仕相傌俥炮兵將士象馬車砲卒"
    # numerals 1-9 (Chinese) + move words
    "一二三四五六七八九进退平前中后"
    # river text
    "楚河漢界"
    # simplified variants (notation parser/render accepts both)
    "帅将仕相马车炮兵卒进"
)
book = root / "crates/xq-book/data/openings.json"
text = book.read_text(encoding="utf-8") if book.exists() else ""
extra = {c for c in text if "\u4e00" <= c <= "\u9fff" or "\u3400" <= c <= "\u4dbf" or "\uf900" <= c <= "\ufaff"}
chars = sorted(set(fixed) | extra)
out = root / "tools/cjk_chars.txt"
out.write_text("".join(chars) + "\n", encoding="utf-8")
print(f"cjk_chars.txt: {len(chars)} chars")
PYEOF

# 2. Extract the SC face (index 2) from each .ttc.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
python3 - "$SERIF_BOLD_TTC" "$SERIF_REG_TTC" "$TMP" <<'PYEOF'
import sys
from fontTools.ttLib import TTCollection
for ttc, name in [(sys.argv[1], "serif-bold.ttf"), (sys.argv[2], "serif-regular.ttf")]:
    fonts = TTCollection(ttc).fonts
    # Face order in Noto CJK .ttc: JP, KR, SC, TC, HK -> SC is index 2.
    sc = next((f for f in fonts if f["name"].getDebugName(1) == "Noto Serif CJK SC"), fonts[2])
    sc.save(f"{sys.argv[3]}/{name}")
    print(f"extracted SC face from {ttc}")
PYEOF

# 3. Subset the CJK faces with the char list.
python3 -m fontTools.subset "$TMP/serif-bold.ttf" \
    --text-file="$ROOT/tools/cjk_chars.txt" \
    --output-file="$OUT/NotoSerifCJKSC-Bold-subset.ttf" \
    --layout-features=kern,liga --no-hinting
python3 -m fontTools.subset "$TMP/serif-regular.ttf" \
    --text-file="$ROOT/tools/cjk_chars.txt" \
    --output-file="$OUT/NotoSerifCJKSC-Regular-subset.ttf" \
    --layout-features=kern,liga --no-hinting

# 4. Subset Noto Sans for Vietnamese + Latin (UI font, SS7.4).
VI_UNICODES="U+0020-007E,U+00A0-00FF,U+0100-024F,U+1E00-1EFF,U+2010-2027,U+2030-203A,U+20AB"
python3 -m fontTools.subset "$SANS_REG" \
    --unicodes="$VI_UNICODES" \
    --output-file="$OUT/NotoSans-Regular-subset.ttf" \
    --layout-features=kern,liga --no-hinting
python3 -m fontTools.subset "$SANS_BOLD" \
    --unicodes="$VI_UNICODES" \
    --output-file="$OUT/NotoSans-Bold-subset.ttf" \
    --layout-features=kern,liga --no-hinting

ls -l "$OUT"
