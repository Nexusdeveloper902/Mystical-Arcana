#!/usr/bin/env python3
"""
Split docs/mystical-arcana-design.md into per-system files under docs/.

Mapping is curated: related sections are grouped into one system file so each
file is a coherent subsystem spec rather than a fragment per heading.
"""

from pathlib import Path
import re

SRC = Path("/home/z/my-project/docs/mystical-arcana-design.md")
OUT_DIR = Path("/home/z/my-project/docs/systems")
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Map: source-section-number -> output-file-basename
SECTION_MAP = {
    1:  "00-overview.md",
    2:  "01-philosophy.md",
    3:  "02-visual-identity.md",
    4:  "02-visual-identity.md",
    5:  "02-visual-identity.md",
    6:  "03-runes.md",
    7:  "04-player.md",
    8:  "05-world.md",
    9:  "05-world.md",
    10: "05-world.md",
    11: "05-world.md",
    12: "06-mana.md",
    13: "06-mana.md",
    14: "07-inventory.md",
    15: "03-runes.md",
    16: "03-runes.md",
    17: "03-runes.md",
    18: "08-crafting-research.md",
    19: "09-magic.md",
    20: "09-magic.md",
    21: "10-resources.md",
    22: "11-combat.md",
    23: "11-combat.md",
    24: "11-combat.md",
    25: "12-building.md",
    26: "12-building.md",
    27: "08-crafting-research.md",
    28: "08-crafting-research.md",
    29: "13-progression.md",
    30: "14-exploration.md",
    31: "14-exploration.md",
    37: "14-exploration.md",
    32: "15-ui.md",
    33: "03-runes.md",
    34: "16-vfx.md",
    35: "17-sound-atmosphere.md",
    36: "17-sound-atmosphere.md",
    38: "18-persistence.md",
    39: "19-accessibility.md",
    40: "20-technical.md",
    41: "20-technical.md",
    42: "21-anti-patterns.md",
    43: "22-player-experience.md",
    44: "22-player-experience.md",
    45: "23-vision.md",
    46: "24-feature-ecosystem.md",
    47: "25-principles.md",
    48: "25-principles.md",
    49: "25-principles.md",
}

FILE_TITLES = {
    "00-overview.md":              "Overview — What Mystical Arcana Is",
    "01-philosophy.md":            "Central Design Philosophy",
    "02-visual-identity.md":       "Visual Identity (Color, Lighting, Stylized Look)",
    "03-runes.md":                 "Runes — Visual Language, System, Combinations, Tablets, Iconography",
    "04-player.md":                "The Arcanist — Player Identity",
    "05-world.md":                 "World — Regions, Procedural Generation, Ley Lines, Mana Nodes",
    "06-mana.md":                  "Mana Ecosystem & Mana Burn",
    "07-inventory.md":             "Inventory",
    "08-crafting-research.md":     "Schematics, Crafting & Research",
    "09-magic.md":                 "Spellcasting & Magic Replaces Tools",
    "10-resources.md":             "Resources & Gathering Economy",
    "11-combat.md":                "Combat, Enemies & Mana Corruption",
    "12-building.md":              "Base Building & Sanctuaries",
    "13-progression.md":          "Progression",
    "14-exploration.md":          "Exploration Loop, Mystery & Environmental Storytelling",
    "15-ui.md":                    "UI Identity",
    "16-vfx.md":                   "VFX Identity",
    "17-sound-atmosphere.md":      "Sound Identity & Atmosphere",
    "18-persistence.md":           "Persistence & Save System",
    "19-accessibility.md":         "Accessibility & Usability",
    "20-technical.md":             "Technical Identity & Performance Philosophy",
    "21-anti-patterns.md":        "Anti-Patterns — What the Game Should NOT Become",
    "22-player-experience.md":     "Ideal Player Experience & Ultimate Gameplay Fantasy",
    "23-vision.md":                "Long-Term Vision",
    "24-feature-ecosystem.md":     "Complete Feature Ecosystem",
    "25-principles.md":            "Principles — North Star & Final Statement",
}

def parse_sections(text: str):
    lines = text.split("\n")
    section_starts = []
    pat = re.compile(r"^#{1,2}\s+(\d+)\.\s")
    for i, line in enumerate(lines):
        m = pat.match(line)
        if m:
            section_starts.append((int(m.group(1)), i))
    out = []
    for idx, (num, start) in enumerate(section_starts):
        end = section_starts[idx + 1][1] if idx + 1 < len(section_starts) else len(lines)
        body = "\n".join(lines[start:end]).rstrip() + "\n"
        out.append((num, body))
    return out

def main():
    text = SRC.read_text(encoding="utf-8")
    sections = parse_sections(text)
    print(f"Parsed {len(sections)} sections")

    buckets = {}
    for num, body in sections:
        target = SECTION_MAP.get(num)
        if target is None:
            print(f"  WARN: section {num} has no target mapping, skipping")
            continue
        buckets.setdefault(target, []).append((num, body))

    written = []
    for fname, secs in sorted(buckets.items()):
        title = FILE_TITLES.get(fname, fname)
        out_path = OUT_DIR / fname
        secs_sorted = sorted(secs, key=lambda x: x[0])
        parts = []
        parts.append(f"# {title}\n")
        parts.append(f"> Subsystem spec — derived from `mystical-arcana-design.md`.\n")
        parts.append(f"> Sections covered: {', '.join(str(n) for n, _ in secs_sorted)}.\n")
        parts.append("\n---\n\n")
        for num, body in secs_sorted:
            stripped = re.sub(r"^(#{1,2})\s+\d+\.\s", r"\1 ", body, count=1, flags=re.MULTILINE)
            parts.append(stripped.rstrip() + "\n")
            parts.append("\n---\n")
        out_path.write_text("\n".join(parts), encoding="utf-8")
        written.append((fname, len(secs_sorted), out_path.stat().st_size))
        print(f"  wrote {out_path}  ({len(secs_sorted)} sections, {out_path.stat().st_size} bytes)")

    archive_dir = SRC.parent / "_archive"
    archive_dir.mkdir(exist_ok=True)
    archive_path = archive_dir / "mystical-arcana-design.md.bak"
    archive_path.write_text(text, encoding="utf-8")
    print(f"  archived original to {archive_path}")

    index_lines = [
        "# Mystical Arcana — Design Documentation Index\n",
        "> This file was the single-file creative & product direction. It has been",
        "> split into per-subsystem files under [`systems/`](./systems/). The",
        "> original monolithic file is preserved at",
        "> [`_archive/mystical-arcana-design.md.bak`](./_archive/mystical-arcana-design.md.bak)\n",
        "## Subsystem files\n",
        "| File | Subsystem | Sections |",
        "|------|-----------|---------:|",
    ]
    for fname, count, size in written:
        title = FILE_TITLES.get(fname, fname)
        index_lines.append(f"| [`systems/{fname}`](./systems/{fname}) | {title} | {count} |")
    index_lines += [
        "",
        "## How to read this documentation",
        "",
        "1. Start with [`systems/00-overview.md`](./systems/00-overview.md) — the core premise.",
        "2. Then [`systems/01-philosophy.md`](./systems/01-philosophy.md) — the four pillars.",
        "3. Then read the subsystem that interests you. The feature ecosystem",
        "   ([`systems/24-feature-ecosystem.md`](./systems/24-feature-ecosystem.md)) is the",
        "   best map of the whole project at a glance.",
        "4. Principles ([`systems/25-principles.md`](./systems/25-principles.md)) should be",
        "   consulted whenever a design decision is in doubt.",
        "",
    ]
    SRC.write_text("\n".join(index_lines), encoding="utf-8")
    print(f"  rewrote {SRC} as index")

    print(f"\nDONE: {len(written)} subsystem files written, original archived, index installed.")

if __name__ == "__main__":
    main()
