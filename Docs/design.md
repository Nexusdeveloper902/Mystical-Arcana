# Mystical Arcana — Creative & Product Direction (Canonical Summary)

> The full design document lives in the project's conversation history. This file is the engineering-facing summary; it captures the design intent that every code decision must reinforce.

## Premise

The player is an **Arcanist** — a former craftsman whose body developed a condition that causes ordinary instruments to decay on touch. Forced to abandon conventional tools, they turn to the forgotten Arcanum. Their replacement for tools is **will**. Their medium is **mana**. Their language is **runes**. Their laboratory is **the world**.

## Four pillars

1. **Magic is a language.** Runes are concepts (Movement, Transformation, Protection, Destruction, Manipulation). Combinations are sentences.
2. **Mana is an ecosystem.** Mana saturates the world, shapes environments, mutates creatures, and destabilizes the careless.
3. **Knowledge is progression.** The player becomes stronger because they understand more — not because their stats went up.
4. **The world generates the adventure.** No quest markers. The world's magical geography (ley lines, anomalies, corruption, ruins) creates reasons to explore.

## Central fantasy

> **The player progresses from survival → discovery → understanding → manipulation → mastery.**

The cost: power is not necessarily good; knowledge is not necessarily safe. Mana can reshape the Arcanist's body and mind — toward ascension or corruption.

## Visual identity

- Stylized mystical fantasy, **not photorealistic**.
- Natural earthy foundation: stone, dirt, wood, leaves, grass, water, crystals.
- Controlled magical colors: earth → vegetation → stone → water → crystal → mana (not neon-everything).
- Mana glows communicate energy and life, not overexposure.
- Greek-inspired runic symbols form a coherent written language — recognizable across UI, VFX, tablets, ruins, sanctuary architecture.

## World layers (vertical progression)

1. **Surface** — forests, plains, rivers, mountains, basic resources, relatively safe.
2. **Subterranean / crystalline** — caves, mana deposits, crystalline formations, stronger phenomena, more dangerous creatures.
3. **Deep regions** — bioluminescent environments, highly concentrated mana, corruption, unstable physics, powerful enemies.
4. **Ancient sanctuaries / floating regions** — remnants of old Arcanists, advanced magical architecture, lost knowledge.

## Lighting contrast

- Surface: soft natural sunlight, long shadows, fog, subtle magical illumination.
- Underground: increasingly dependent on crystals, mana deposits, bioluminescence.
- High-mana: lighting becomes unnatural; the player should *feel* that something is wrong before understanding why.
- Sanctuaries: warm, stable, controlled, rhythmic — the opposite of the chaotic wilderness.

## Core systems

| System | Identity |
|---|---|
| **Mana pool + Mana Burn** | Power vs stability — overuse scars the player and destabilizes the environment |
| **Mana Nodes** | Proximity regen + ambient hum; not recharge stations, magical network nodes |
| **Ley lines** | Spline-represented invisible rivers of mana, partially visible when concentration is high |
| **Runes** | Data-driven, Greek-inspired, share identity across UI/world/VFX/tablets/research |
| **Rune combinations → Schematics** | Improvisation (experiment) vs Knowledge (known schematic) — compositional magic |
| **Rune tablets** | Placeable runes — magic becomes spatial, not just inventory |
| **Spellcasting** | Gestures, symbols, motion, light, particles, sound — alive, not bullets |
| **Magic-as-tool** | Gather Bolt extracts resources; the player replaces axe→tree with understand→matter |
| **Inventory** | Subordinate to magical identity — carries ingredients to interact with the world, not a spreadsheet |
| **Combat** | Magical manipulation, not firearms-with-fantasy-art |
| **Enemies + AI** | Modular behavior; corruption mutates strength, behavior, appearance |
| **Building** | "Stabilizing reality," not placing prefabricated walls |
| **Sanctuaries** | Storage, crafting, research, spell work, regen, magical stability, protection |
| **Research** | Unlocks runes, discovers schematics, upgrades capabilities — affects gameplay |

## Engineering filter

> Does this system reinforce the idea that the player is learning to understand and manipulate the magical world?

If yes, build it. If no, reconsider.

## Forbidden patterns

- Generic neon fantasy.
- Default-engine visual identity (no Unity look, no Unreal look, no Godot look).
- Linear quests and rigid missions.
- Stats-based progression without knowledge acquisition.
- Tools-as-weapons with magical art (magic should feel like magic).
