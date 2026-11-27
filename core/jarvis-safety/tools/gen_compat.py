#!/usr/bin/env python3
"""Review pass 38 (extended in review pass 39): generates the tables of `src/compat.rs`,
`tests/generated/assigned.rs` and `tests/generated/emoji.rs` from Unicode 15.1.

Run from `core/jarvis-safety`:  python3 tools/gen_compat.py <ucd-dir>

Sources (development time only; nothing here is a runtime dependency):

* Python's `unicodedata` (it must report Unicode 15.1.0, the version of `norm.rs`):
  decompositions, General_Category.
* Review pass 39: three files of the Unicode Character Database 15.1.0, downloaded into
  `<ucd-dir>` from https://www.unicode.org/Public/15.1.0/ucd/ (checked by SHA-256 below):
  `emoji/emoji-data.txt` (Extended_Pictographic, Emoji_Presentation), `Scripts.txt`
  (Script), `PropList.txt` (Prepended_Concatenation_Mark).

Tables:

* COMPAT_FOLD: every code point outside ASCII whose compatibility decomposition (NFKD),
  with every character that is neither a letter (General_Category L*) nor a decimal digit
  (Nd) removed, is exactly one ASCII letter or one ASCII digit, mapped to it (parenthesized
  and circled letters and digits, fullwidth and mathematical forms, squared and enclosed
  letters, modifier, superscript and subscript letters and digits, Roman numeral one, the
  long s ...). A test in `norm.rs` recomputes the count and the FNV-1a hash written here.
* Review pass 39: COMPAT_LETTERS: every code point of General_Category L*, So or Nl, not
  in COMPAT_FOLD, whose NFKD with every non-letter removed is two or more ASCII letters,
  mapped to them (`㏒` log, `㎈` cal, `㏌` in, `ﬁ` fi, `ǅ` Dz, `Ⅱ` II, `℡` TEL). Not
  taken: Extended_Pictographic code points (emoji stay emoji: `™`, `🆗`), currency signs
  (Sc: `₨`), and the numero sign `№` (word punctuation). Count and hash checked as above.
* Review pass 39: EMOJI_RANGES (replaces pass 38's PICTOGRAPH_RANGES, an approximation by
  So + East_Asian_Width W that let CJK strokes and radicals in): every assigned code point
  of emoji-data.txt 15.1.0 that is Emoji_Presentation, or both Emoji and
  Extended_Pictographic (real emoji: not the keycap bases, and not the pictographs
  reserved for future emoji, such as `⚬` U+26AC or `☐` U+2610, which are not Emoji).
* Review pass 39: SCRIPT_RANGES: every letter (General_Category L*) by Script, as a family:
  0 Latin, 1 Greek, 2 Cyrillic, 3 Armenian, 4 Cherokee, 5 Coptic, 6 Lisu, 7 Georgian,
  8 the CJK family (Han, Hiragana, Katakana, Hangul, Bopomofo: one family, since they are
  written together), then one number per other script in order of first appearance.
  Common and Inherited letters are left out (no script).
* Review pass 39: PREPENDED_CONCATENATION_MARKS: the format characters a font draws (they
  are read as drawn characters, not dropped).
* ASSIGNED: every code point whose General_Category is not Cn (the every-code-point test of
  `tests/pass38_fixes.rs` walks them), as ranges; EMOJI (tests): EMOJI_RANGES again.
"""
import hashlib
import os
import sys
import unicodedata as u

assert u.unidata_version == "15.1.0", u.unidata_version

UCD = sys.argv[1] if len(sys.argv) > 1 else "ucd"
SHA256 = {
    "emoji-data.txt": "d7aef489c8fe4c14f09ea5695200277c6b93ac82ac60845cdd2161b0d6835cc1",
    "Scripts.txt": "0eacb65169ae6eb1d399cd70826b3da15fff19f6f586eecf819b70c83b1d9b32",
    "PropList.txt": "05672956317b6296bc2ec3d6cef1f6452b57ff4f2efc6dc55b0a19277d5fcfd1",
}


def ucd_lines(name):
    path = os.path.join(UCD, name)
    with open(path, "rb") as f:
        data = f.read()
    got = hashlib.sha256(data).hexdigest()
    assert got == SHA256[name], f"{name}: sha256 {got}"
    for line in data.decode("utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        cps, prop = [x.strip() for x in line.split(";")[:2]]
        if ".." in cps:
            a, b = cps.split("..")
            yield int(a, 16), int(b, 16), prop
        else:
            yield int(cps, 16), int(cps, 16), prop


def ranges(cps):
    out = []
    for c in cps:
        if out and out[-1][1] == c - 1:
            out[-1][1] = c
        else:
            out.append([c, c])
    return out


def scalars(lo):
    for cp in range(lo, 0x110000):
        if not 0xD800 <= cp <= 0xDFFF:
            yield cp


def fnv(entries):
    h = 0xCBF29CE484222325
    for cp, a in entries:
        for b in f"{cp}:{a};".encode():
            h ^= b
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


props = {"Emoji": set(), "Extended_Pictographic": set(), "Emoji_Presentation": set()}
for a, b, prop in ucd_lines("emoji-data.txt"):
    if prop in props:
        props[prop].update(range(a, b + 1))
ext_pict = props["Extended_Pictographic"]
# Real emoji: Emoji_Presentation, or Emoji and Extended_Pictographic (not the keycap bases
# `#`, `*`, digits; not the pictographs Unicode reserves for future emoji, `⚬` `☐`).
emoji_set = {
    cp
    for cp in (props["Emoji"] & ext_pict) | props["Emoji_Presentation"]
    if u.category(chr(cp)) != "Cn"
}

pcm = []
for a, b, prop in ucd_lines("PropList.txt"):
    if prop == "Prepended_Concatenation_Mark":
        pcm.extend(range(a, b + 1))
pcm.sort()

script_of = {}
for a, b, prop in ucd_lines("Scripts.txt"):
    for cp in range(a, b + 1):
        script_of[cp] = prop

fold = []
letters = []
for cp in scalars(0x80):
    ch = chr(cp)
    d = u.normalize("NFKD", ch)
    if d == ch:
        continue
    kept = [x for x in d if u.category(x).startswith("L") or u.category(x) == "Nd"]
    if len(kept) == 1 and kept[0].isascii() and kept[0].isalnum():
        fold.append((cp, ord(kept[0])))
        continue
    cat = u.category(ch)
    if not (cat.startswith("L") or cat in ("So", "Nl")):
        continue
    if cp in ext_pict or cp == 0x2116:
        continue
    only = "".join(x for x in d if u.category(x).startswith("L"))
    if len(only) >= 2 and only.isascii() and only.isalpha():
        letters.append((cp, only))

h = fnv(fold)
hl = fnv(letters)
# Review pass 39: the spans the multi-letter forms lie in (entries less than 64 apart
# joined): a character outside them is looked up without the binary search.
spans = []
for cp, _ in letters:
    if spans and cp - spans[-1][1] <= 64:
        spans[-1][1] = cp
    else:
        spans.append([cp, cp])

er = ranges(sorted(emoji_set))

FIXED = {
    "Latin": 0,
    "Greek": 1,
    "Cyrillic": 2,
    "Armenian": 3,
    "Cherokee": 4,
    "Coptic": 5,
    "Lisu": 6,
    "Georgian": 7,
    "Han": 8,
    "Hiragana": 8,
    "Katakana": 8,
    "Hangul": 8,
    "Bopomofo": 8,
}
family = dict(FIXED)
script_ranges = []
for cp in scalars(0):
    ch = chr(cp)
    if not u.category(ch).startswith("L"):
        continue
    sc = script_of.get(cp, "Unknown")
    if sc in ("Common", "Inherited", "Unknown"):
        continue
    if sc not in family:
        family[sc] = max(family.values()) + 1
    fam = family[sc]
    if script_ranges and script_ranges[-1][1] == cp - 1 and script_ranges[-1][2] == fam:
        script_ranges[-1][1] = cp
    else:
        script_ranges.append([cp, cp, fam])

assigned = [cp for cp in scalars(0) if u.category(chr(cp)) != "Cn"]
ar = ranges(assigned)

HEAD = (
    "//! Review pass 38 (extended in review pass 39): tables generated by\n"
    "//! `tools/gen_compat.py` from Unicode 15.1 (Python's `unicodedata` 15.1.0; the UCD\n"
    "//! 15.1.0 files emoji-data.txt, Scripts.txt and PropList.txt). Do not edit by hand:\n"
    "//! rerun the script.\n\n"
)

with open("src/compat.rs", "w") as f:
    f.write(HEAD)
    f.write("/// Code points whose compatibility decomposition holds exactly one letter or digit,\n")
    f.write("/// ASCII, and nothing else but non-letters: (code point, that ASCII character).\n")
    f.write(f"pub(crate) const COMPAT_FOLD: [(u32, u8); {len(fold)}] = [\n")
    for cp, a in fold:
        f.write(f"    (0x{cp:X}, b'{chr(a)}'),\n")
    f.write("];\n\n")
    f.write("/// How many entries the recipe gives (checked by a test).\n")
    f.write("#[cfg(test)]\n")
    f.write(f"pub(crate) const COMPAT_FOLD_LEN: usize = {len(fold)};\n")
    f.write("/// FNV-1a 64 of `\"{cp}:{ascii};\"` (decimal) over the entries in order.\n")
    f.write("#[cfg(test)]\n")
    f.write(f"pub(crate) const COMPAT_FOLD_HASH: u64 = 0x{h:016X};\n\n")
    f.write("/// Review pass 39: code points (letters, other symbols, letter numbers) whose\n")
    f.write("/// compatibility decomposition, without its non-letters, is two or more ASCII\n")
    f.write("/// letters: (code point, those letters). Emoji, currency signs and `№` left out.\n")
    f.write(f"pub(crate) const COMPAT_LETTERS: [(u32, &str); {len(letters)}] = [\n")
    for cp, s in letters:
        f.write(f"    (0x{cp:X}, \"{s}\"),\n")
    f.write("];\n\n")
    f.write("/// Review pass 39: the spans COMPAT_LETTERS lies in (entries less than 64 apart\n")
    f.write("/// joined), so most characters need no binary search.\n")
    f.write(f"pub(crate) const COMPAT_LETTERS_SPANS: [(u32, u32); {len(spans)}] = [\n")
    for a, b in spans:
        f.write(f"    (0x{a:X}, 0x{b:X}),\n")
    f.write("];\n\n")
    f.write("/// How many entries the recipe gives (checked by a test).\n")
    f.write("#[cfg(test)]\n")
    f.write(f"pub(crate) const COMPAT_LETTERS_LEN: usize = {len(letters)};\n")
    f.write("/// FNV-1a 64 of `\"{cp}:{letters};\"` (decimal) over the entries in order.\n")
    f.write("#[cfg(test)]\n")
    f.write(f"pub(crate) const COMPAT_LETTERS_HASH: u64 = 0x{hl:016X};\n\n")
    f.write("/// Review pass 39: the emoji (assigned code points of emoji-data.txt 15.1.0 that are\n")
    f.write("/// Emoji_Presentation, or Emoji and Extended_Pictographic; ")
    f.write(f"{len(emoji_set)} code points),\n")
    f.write("/// sorted inclusive ranges: read as an icon at a word's edge.\n")
    f.write(f"pub(crate) const EMOJI_RANGES: [(u32, u32); {len(er)}] = [\n")
    for a, b in er:
        f.write(f"    (0x{a:X}, 0x{b:X}),\n")
    f.write("];\n\n")
    f.write("/// Review pass 39: every letter's script family (Scripts.txt 15.1.0): 0 Latin, 1 Greek,\n")
    f.write("/// 2 Cyrillic, 3 Armenian, 4 Cherokee, 5 Coptic, 6 Lisu, 7 Georgian, 8 Han, Hiragana,\n")
    f.write("/// Katakana, Hangul and Bopomofo, then one per other script; Common and Inherited\n")
    f.write("/// letters are not listed. Sorted inclusive ranges (low, high, family).\n")
    f.write(f"pub(crate) const SCRIPT_RANGES: [(u32, u32, u8); {len(script_ranges)}] = [\n")
    for a, b, fam in script_ranges:
        f.write(f"    (0x{a:X}, 0x{b:X}, {fam}),\n")
    f.write("];\n\n")
    f.write("/// Review pass 39: Prepended_Concatenation_Mark (PropList.txt 15.1.0): format\n")
    f.write("/// characters a font draws.\n")
    f.write(f"pub(crate) const PREPENDED_CONCATENATION_MARKS: [u32; {len(pcm)}] = [\n")
    for cp in pcm:
        f.write(f"    0x{cp:X},\n")
    f.write("];\n")

os.makedirs("tests/generated", exist_ok=True)
with open("tests/generated/assigned.rs", "w") as f:
    f.write("// Review pass 38: generated by `tools/gen_compat.py` from Unicode 15.1 (Python's\n")
    f.write("// `unicodedata` 15.1.0): every assigned code point (General_Category not Cn).\n\n")
    f.write(f"/// {len(assigned)} assigned code points.\n")
    f.write(f"pub const ASSIGNED_COUNT: u32 = {len(assigned)};\n")
    f.write("/// The assigned code points as sorted inclusive ranges.\n")
    f.write(f"pub const ASSIGNED: [(u32, u32); {len(ar)}] = [\n")
    for a, b in ar:
        f.write(f"    (0x{a:X}, 0x{b:X}),\n")
    f.write("];\n")
with open("tests/generated/emoji.rs", "w") as f:
    f.write("// Review pass 39: generated by `tools/gen_compat.py` from Unicode 15.1 (emoji-data.txt\n")
    f.write("// 15.1.0): every assigned code point that is Emoji_Presentation, or Emoji and\n")
    f.write("// Extended_Pictographic.\n\n")
    f.write(f"/// {len(emoji_set)} emoji code points.\n")
    f.write(f"pub const EMOJI_COUNT: u32 = {len(emoji_set)};\n")
    f.write("/// The emoji as sorted inclusive ranges.\n")
    f.write(f"pub const EMOJI: [(u32, u32); {len(er)}] = [\n")
    for a, b in er:
        f.write(f"    (0x{a:X}, 0x{b:X}),\n")
    f.write("];\n")
print("fold", len(fold), hex(h), "letters", len(letters), hex(hl), "emoji", len(er), len(emoji_set),
      "scripts", len(script_ranges), len(family), "pcm", len(pcm), "assigned", len(assigned), len(ar))
