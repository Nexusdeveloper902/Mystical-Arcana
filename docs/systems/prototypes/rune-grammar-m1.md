# Prototype Spec — Rune Grammar & Spellcasting

This is an implementation-ready expansion of [`03-runes.md`](../03-runes.md)
and [`09-magic.md`](../09-magic.md), scoped to **Milestone M1**
([`roadmap.md`](../../roadmap.md)). It defines the data model, the input loop,
and the validation rules so an engineer can start writing code without
re-deriving the design.

---

## 1. Goals (M1 scope)

- 10 runes total (5 starter from M0 + 5 new from M1.4).
- 15 valid combinations producing 15 distinct spells.
- Tracing input that is forgiving but readable.
- A research interface that lets players *discover* combinations rather than
  be told them.

Non-goals (deferred to M2+):

- Conditional combinations that depend on environment (e.g., `WARM + GROW`
  only works in stable mana fields).
- Stack-time multipliers (e.g., drawing `MOVE` twice for a longer dash).
- Player-defined rune aliases or macros.

---

## 2. Rune data model

Each rune is a ScriptableObject (or plain JSON asset if not on Unity yet):

```yaml
rune:
  id: MOVE
  display_name: "Move"
  icon: assets/icons/runes/move.svg
  glyph_path: "M 0 0 L 12 0 L 6 12 Z"   # SVG-style path for tracing UX
  base_mana_cost: 1.0                     # in mana units
  stability: 0.8                          # 0..1, used by burn model
  concept: "Motion"                       # human-readable concept tag
  element: None                            # one of: None, Fire, Water, Earth, Air, Void, Aether
  opposites: [STILL]                       # runes that conceptually negate this one
  tags: [motion, basic]
```

The full M1 rune set:

| Rune | Concept | Element | Notes |
|------|---------|---------|-------|
| `MOVE` | Motion | None | Starter (M0.3) |
| `SHAPE` | Form | Earth | Starter |
| `WARM` | Heat | Fire | Starter |
| `COOL` | Cold | Water | Starter |
| `GLOW` | Light | Aether | Starter |
| `BIND` | Cohesion | Earth | M1.4 |
| `SHATTER` | Disruption | Void | M1.4 |
| `GROW` | Increase | Aether | M1.4 |
| `STILL` | Stasis | Water | M1.4 |
| `WARD` | Protection | Air | M1.4 |

---

## 3. Spell = validated combination

A spell is the *output* of a combination, not the input. Combinations are
declared as 2-rune tuples (order matters for some, not for others):

```yaml
combination:
  runes: [MOVE, SHAPE]
  order_matters: false
  result_spell:
    id: SPELL_LIFT
    display_name: "Lift"
    description: "Raise a small mass of material to eye level."
    mana_cost: 2.5              # overrides sum of base costs
    cast_time_ms: 600
    cooldown_ms: 1500
    effects:
      - type: transform_target
        transform: translate_y
        amount: 1.5             # meters
        duration_s: 5
    vfx: assets/vfx/spells/lift.prefab
    sfx: assets/sfx/spells/lift.wav
```

### 3.1 Order-sensitivity rules

Combinations where one rune is `BIND`, `SHATTER`, `WARD`, or `STILL` are
order-sensitive: the second rune is the *target* the first acts on.

| Combination (in order) | Result | Why |
|------------------------|--------|-----|
| `BIND + MOVE` | "Hold in place" | Bind locks the motion concept |
| `MOVE + BIND` | invalid — the verb `MOVE` needs a target concept, not another verb | raises `InvalidCombinationError` |

Combinations of two *concept* runes (e.g., `WARM + GROW`) are order-insensitive.

---

## 4. The 15 M1 combinations

| # | Runes (canonical order) | Result spell | Effect summary |
|---|--------------------------|---------------|-----------------|
| 1 | `MOVE + SHAPE` | Lift | Raise mass to eye level |
| 2 | `MOVE + WARM` | Drift Up | Warm air lifts player ~1 m |
| 3 | `MOVE + COOL` | Drift Down | Cool air sinks player ~1 m |
| 4 | `MOVE + GLOW` | Spark Bolt | Launch a short-lived light projectile |
| 5 | `SHAPE + WARM` | Soften | Heat a target until malleable |
| 6 | `SHAPE + COOL` | Harden | Cool a target until rigid |
| 7 | `SHAPE + GROW` | Reshape | Player edits target geometry |
| 8 | `WARM + COOL` | Equalize | Cancel thermal differential in field |
| 9 | `WARM + GLOW` | Emberlight | Persistent low light + minor heat |
| 10 | `COOL + GLOW` | Frostlight | Persistent low light + minor chill |
| 11 | `BIND + MOVE` | Anchor | Hold a body in space |
| 12 | `BIND + SHAPE` | Lockform | Preserve target shape |
| 13 | `SHATTER + SHAPE` | Crumble | Destroy shape of target |
| 14 | `GROW + GLOW` | Bloomlight | Expanding light sphere |
| 15 | `WARD + STILL` | Quiet Field | Suppress mana events in radius |

Invalid pairs (e.g., `MOVE + BIND`): see §3.1.

---

## 5. Input loop

State machine:

```
[IDLE]
   │ player presses Trace (RMB)
   ▼
[TRACING]
   │ stroke sampled into normalized path, matched to a rune's glyph_path
   │   - if match confidence >= 0.6: emit RUNE_ENTERED event
   │   - if confidence < 0.6: emit RUNE_REJECTED, return to TRACING
   ▼
[COMPOSING]
   │ active combination so far: [r1] (optionally [r1, r2])
   │ player either:
   │   - traces another rune -> COMPOSING or VALIDATION_FAIL
   │   - presses Release (LMB) -> VALIDATE
   │   - presses Cancel (Esc) -> IDLE
   ▼
[VALIDATE]
   │ look up (r1, r2) or (r2, r1) in combination table
   │   - if found and order matches order_matters rules -> CAST_SPELL
   │   - if found but order violated -> SPELL_FIZZLE (refund 50% mana)
   │   - if not found -> SPELL_FIZZLE (refund 25% mana, mark "unknown" for research)
   ▼
[CAST_SPELL]
   │ deduct mana_cost, spawn vfx/sfx, dispatch effects
   │ on completion -> IDLE
```

### 5.1 Tracing UX

- **Glyph path matching**: cosine similarity between the player's stroke
  (resampled to 32 points, normalized for translation/scale) and the rune's
  canonical glyph path. Threshold 0.6 for M1; can be tuned per accessibility
  setting ([`19-accessibility.md`](../19-accessibility.md)).
- **Visual feedback**: as the player traces, a glowing trail follows the
  pointer. On match, the trail snaps to the canonical glyph and pulses.
- **Haptic feedback** (if supported): short pulse on match, double-pulse on
  rejection.

### 5.2 Composition feedback

- A small rune strip at the bottom shows the runes entered so far, left to
  right.
- Hovering a placed rune shows its name and concept tag.
- If the active pair is invalid (per §3.1), the strip is outlined red and the
  Release button is suppressed with a tooltip: "Order matters for this pair."

---

## 6. Research interface (M1.3)

The research table is a placeable sanctuary object (M4.4 base-building). It
provides a sandbox where the player can drag any two known runes into two
slots and press **Test**.

- If the combination is in the table (§4): the table reveals the resulting
  spell, adds it to the player's known-spell list, and the schematic is added
  to the player's inventory as a craftable item.
- If the combination is invalid by order rule (§3.1): the table says
  "Order matters here. Try the reverse." and reveals nothing.
- If the combination is not in the table at all: the table says "Nothing
  resonates. Try other runes." and adds the pair to the player's
  *failed-attempts* log (used later for hint generation in M2).

The table is **not** required to discover spells — a player who simply casts
an unknown pair in the wild still discovers it (with the mana penalty of §5).
The table is the *safe* path.

---

## 7. Mana cost & burn interaction

Per [`06-mana.md`](../06-mana.md):

- Each rune has a `base_mana_cost`. Each combination has an override
  `mana_cost` (usually lower than the sum, to reward combination play).
- If the player's current mana pool is below `mana_cost` at VALIDATE time,
  the spell fizzles and **does not** refund.
- Each spell cast raises the player's `mana_burn` meter by a small amount
  proportional to the spell's stability delta (`1 - min(stability of r1, r2)`).
  At burn meter >= 0.75, visual blur + audio hum begin.

---

## 8. Edge cases

- **Same rune twice** (e.g., `MOVE + MOVE`): not valid in M1. Surface as
  "Stacking is reserved for a later update." tooltip.
- **Trace aborted**: stroke that never reaches confidence threshold within
  4 seconds auto-cancels back to IDLE.
- **Save/restore mid-composition**: not supported in M1. Composition state
  is volatile; if the player saves mid-composition, it is discarded on load.
- **Cross-system interaction**: `Lift` on a `BIND`-anchored body: the BIND
  wins (anchor has higher priority than transform for M1). Documented as
  intended behavior; revisited in M2.

---

## 9. Implementation order

Suggested PR sequence inside M1:

1. PR1 — rune data model + 10 runes as JSON/YAML assets.
2. PR2 — combination table as data + validation logic + tests.
3. PR3 — tracing input + glyph matching (no spells cast yet).
4. PR4 — composition state machine + fizzle paths.
5. PR5 — effect dispatcher + first 5 spells (rows 1–5 of §4).
6. PR6 — remaining 10 spells.
7. PR7 — research table object + UI.

Each PR is independently reviewable and demoable.

---

## 10. Tests

Minimum automated coverage:

- `combination_table_test`: every entry in §4 returns the expected spell id.
- `order_validation_test`: `MOVE + BIND` rejects, `BIND + MOVE` accepts.
- `mana_cost_test`: fizzle when pool < cost; no refund.
- `glyph_match_test`: 32 canonical glyphs each match their canonical path
  with confidence >= 0.95.
- `tracing_e2e_test`: simulated stroke -> rune entered -> composition -> spell cast.

Playtest checklist (manual):

- [ ] Trace 10 runes from cold (no UI hint).
- [ ] Discover all 15 combinations via the research table.
- [ ] Discover at least 3 combinations in the wild (with fizzle penalty).
- [ ] Cast all 15 spells at least once.
- [ ] Trigger mana burn at least once.
