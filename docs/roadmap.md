# Development Roadmap

This file tracks the active development trajectory of Mystical Arcana. It is
ordered by milestone, with each milestone listing the deliverables, acceptance
criteria, and the subsystem spec it draws from.

The roadmap assumes a **Unity URP** target (per the technical identity spec,
[`docs/systems/20-technical.md`](./systems/20-technical.md)). It can be ported
to another engine if the team chooses; the design specs are engine-agnostic.

---

## M0 — Vertical-slice foundation  *(target: 8 weeks)*

Goal: prove the four pillars on a single small island so the team and any
future publisher can *feel* what Mystical Arcana is.

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M0.1 | First-person controller tuned for tactical casting (no sprint, walk + lean) | [`04-player.md`](./systems/04-player.md) | ☐ |
| M0.2 | Mana field on a 64×64 island: density varies, surfaces visualize as faint haze | [`06-mana.md`](./systems/06-mana.md) | ☐ |
| M0.3 | 5 starter runes: `MOVE`, `SHAPE`, `WARM`, `COOL`, `GLOW` | [`03-runes.md`](./systems/03-runes.md) | ☐ |
| M0.4 | Spellcasting input: trace rune → release → effect | [`09-magic.md`](./systems/09-magic.md) | ☐ |
| M0.5 | Mana burn meter + 3 symptoms (visual blur, audio hum, hand tremor) | [`06-mana.md`](./systems/06-mana.md) | ☐ |
| M0.6 | 1 sanctuary tile (regen + clean field) | [`12-building.md`](./systems/12-building.md) | ☐ |
| M0.7 | Procedural island generator (1 biome, 3 POIs) | [`05-world.md`](./systems/05-world.md) | ☐ |
| M0.8 | UI shell: rune tablet, inventory grid, minimal HUD | [`15-ui.md`](./systems/15-ui.md) | ☐ |

### Acceptance

- A new player, given only the rune tablet and 5 runes, can reach a sanctuary
  without dying to mana burn, *without any tutorial text*.
- The session feels "magic-first": at no point is the player asked to use a
  physical tool.

---

## M1 — Rune combination depth  *(target: 6 weeks)*

Goal: prove the "magic is a language" pillar by enabling *compound* spells.

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M1.1 | Rune combination grammar: `RUNE_A + RUNE_B → SPELL_C` (15 combinations) | [`03-runes.md`](./systems/03-runes.md) | ☐ |
| M1.2 | Schematic discovery: 4 starter schematics | [`08-crafting-research.md`](./systems/08-crafting-research.md) | ☐ |
| M1.3 | Research table: drag 2 runes → learn if combination is valid | [`08-crafting-research.md`](./systems/08-crafting-research.md) | ☐ |
| M1.4 | 5 more runes: `BIND`, `SHATTER`, `GROW`, `STILL`, `WARD` | [`03-runes.md`](./systems/03-runes.md) | ☐ |
| M1.5 | Mana-cost model: each rune + combo has cost, sustainability vs. burn | [`06-mana.md`](./systems/06-mana.md) | ☐ |

### Acceptance

- A player can name at least 3 spells they "invented" by combining runes
  rather than being handed them. Knowledge feels earned.

---

## M2 — World & exploration  *(target: 8 weeks)*

Goal: prove the "world generates the adventure" pillar.

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M2.1 | 3 biomes (Forest, Crystal Cavern, Sanctuary Outskirts) | [`05-world.md`](./systems/05-world.md) | ☐ |
| M2.2 | Ley-line system: visualized as faint energy traces between nodes | [`05-world.md`](./systems/05-world.md) | ☐ |
| M2.3 | 5 mana-node variants (stable, volatile, corrupted, dormant, ancient) | [`05-world.md`](./systems/05-world.md) | ☐ |
| M2.4 | 3 enemy archetypes that react to mana density | [`11-combat.md`](./systems/11-combat.md) | ☐ |
| M2.5 | 1 corrupted creature (Phase 1 → Phase 3 transformation) | [`11-combat.md`](./systems/11-combat.md) | ☐ |
| M2.6 | POI generator with embedded environmental storytelling (rune-carved stones, abandoned camps) | [`14-exploration.md`](./systems/14-exploration.md) | ☐ |

### Acceptance

- A returning player who has played M0 + M1 finds the world genuinely
  different each session, but always coherent.

---

## M3 — Combat & resources  *(target: 6 weeks)*

Goal: prove "magic replaces tools" extends to combat and economy.

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M3.1 | Combat verbs: `BIND` (hold), `SHATTER` (burst), `WARD` (block), `GROW` (counter) | [`11-combat.md`](./systems/11-combat.md) | ☐ |
| M3.2 | Resource gathering: mana-tapped crystal, ley-tapped ore, sanctuary herbs | [`10-resources.md`](./systems/10-resources.md) | ☐ |
| M3.3 | Inventory + schematic merge: discovered schematic auto-loads into crafting menu | [`07-inventory.md`](./systems/07-inventory.md), [`08-crafting-research.md`](./systems/08-crafting-research.md) | ☐ |
| M3.4 | Mana corruption meter: high corruption → ambient distortion + slow burn | [`11-combat.md`](./systems/11-combat.md) | ☐ |

### Acceptance

- A player never swings a sword; combat still feels tactical and varied
  across 3+ enemy types.

---

## M4 — Building & persistence  *(target: 6 weeks)*

Goal: prove the "laboratory is the world" pillar.

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M4.1 | Sanctuary tile placement + manual layout editor | [`12-building.md`](./systems/12-building.md) | ☐ |
| M4.2 | Sanctuaries regenerate mana field when player returns | [`12-building.md`](./systems/12-building.md), [`06-mana.md`](./systems/06-mana.md) | ☐ |
| M4.3 | Save system: world seed, player state, discoveries | [`18-persistence.md`](./systems/18-persistence.md) | ☐ |
| M4.4 | 5 base-building schematics (workbench, archive shelf, rune binder, mana stabilizer, ley tap) | [`12-building.md`](./systems/12-building.md) | ☐ |

### Acceptance

- A player can leave a sanctuary, return after exploring elsewhere, and
  everything (including the local mana field) is in the state they left it.

---

## M5 — Polish & accessibility  *(target: 8 weeks)*

Goal: reach the "ideal player experience" defined in
[`22-player-experience.md`](./systems/22-player-experience.md).

### Deliverables

| # | Deliverable | Spec source | Status |
|---|-------------|-------------|--------|
| M5.1 | Color philosophy pass: dominant / secondary / accent enforced per biome | [`02-visual-identity.md`](./systems/02-visual-identity.md) | ☐ |
| M5.2 | Lighting pass: surface / underground / high-mana / sanctuary mood | [`02-visual-identity.md`](./systems/02-visual-identity.md) | ☐ |
| M5.3 | VFX identity pass: rune traces, mana wisps, burn distortion, sanctuary bloom | [`16-vfx.md`](./systems/16-vfx.md) | ☐ |
| M5.4 | Sound identity pass: nature, magic, instability, sanctuary | [`17-sound-atmosphere.md`](./systems/17-sound-atmosphere.md) | ☐ |
| M5.5 | Accessibility: remappable input, color-blind palettes, subtitle system for magical cues, no mandatory twitch mechanics | [`19-accessibility.md`](./systems/19-accessibility.md) | ☐ |
| M5.6 | Performance pass: stable 60 FPS on mid-range PC, scalable to 30 FPS on handheld-class hardware | [`20-technical.md`](./systems/20-technical.md) | ☐ |

### Acceptance

- An external playtester can complete M0→M4 content in a single sitting and
  describe the experience in their own words — without being told the design
  pillars — in a way that maps cleanly onto
  [`22-player-experience.md`](./systems/22-player-experience.md).

---

## M6 — Long-tail systems  *(target: open-ended)*

Systems that turn a vertical slice into a living product.

- World persistence with offline regrowth ([`05-world.md`](./systems/05-world.md))
- Seasonal / ley-cycle events tied to mana node phase shifts
- Mod support: rune grammar exposed as a data format so modders can add runes
  without touching engine code ([`23-vision.md`](./systems/23-vision.md))
- Localization pipeline: all in-world text (runes, archive entries) lives in
  localizable string tables, with rune iconography as the universal fallback
  ([`15-ui.md`](./systems/15-ui.md), [`19-accessibility.md`](./systems/19-accessibility.md))

---

## How to use this roadmap

1. Pick the next ☐ item in the lowest-numbered milestone that isn't fully
   checked off.
2. Open the linked subsystem spec to anchor the design.
3. Implement, then propose a PR that updates the corresponding row from ☐ to ☑.
4. If a row reveals ambiguity in the spec, update the spec *and* the roadmap
   in the same PR. Specs and roadmap move together.
