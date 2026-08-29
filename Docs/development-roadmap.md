# Development Roadmap

This roadmap follows the phased plan in the brief (sections 50–61) and tracks what has been built and what remains. Each phase must end with passing tests on `main`.

## Status legend

- ✅ Done — implemented, tested, committed
- 🟡 In progress — being built on a feature branch
- ⬜ Pending — not started

## Phase 1 — Engine Foundation

| Subsystem | Crate | Status |
|---|---|---|
| Project structure, build, git, branches | repo | ✅ |
| Workspace, Cargo, CMake+Corrosion | repo | ✅ |
| Logging, assertions, error | `arcane_core` | 🟡 |
| Memory pools, allocators | `arcane_core` | 🟡 |
| Threading primitives, job system | `arcane_core` | 🟡 |
| Profiling scopes, frame stats | `arcane_core` | 🟡 |
| Math (vec, mat, quat, color, AABB, frustum) | `arcane_math` | 🟡 |
| ECS (cache-friendly SoA, queries) | `arcane_ecs` | ⬜ |
| Serialization (binary + RON) | `arcane_core` | ⬜ |
| Vulkan init (instance, debug, device) | `arcane_render` | ⬜ |
| Vulkan swapchain (feature-gated) | `arcane_render` | ⬜ |
| Shader pipeline (GLSL → SPIR-V) | `arcane_render` | ⬜ |
| Basic asset pipeline | `arcane_assets` | ⬜ |

## Phase 2 — Minimal Game Runtime

| Subsystem | Crate | Status |
|---|---|---|
| Scene/world representation | `arcane_world` | ⬜ |
| Entity system binding (ECS → game) | `arcane_ecs`, `game` | ⬜ |
| Transforms, camera | `arcane_math`, `arcane_render` | ⬜ |
| First-person controller | `arcane_input`, `game` | ⬜ |
| Mesh rendering | `arcane_render` | ⬜ |
| Materials | `arcane_render` | ⬜ |
| Lighting (sun, ambient, magical) | `arcane_render` | ⬜ |
| Collision queries (rapier3d) | `arcane_physics` | ⬜ |

## Phase 3 — Mystical Core

| Subsystem | Crate | Status |
|---|---|---|
| Mana pool, regen, burn threshold, Mana Burn | `game::mana` | ⬜ |
| Mana Nodes (proximity regen) | `game::mana` | ⬜ |
| Runes (data-driven, Greek-inspired) | `game::runes` | ⬜ |
| Spell architecture (composition, cost, cooldown) | `game::spells` | ⬜ |
| Gather Bolt (resource extraction) | `game::spells` | ⬜ |
| Resource nodes + inventory | `game::inventory`, `game::resources` | ⬜ |

## Phase 4 — Survival and Combat

| Subsystem | Crate | Status |
|---|---|---|
| Enemies (data-driven defs) | `game::enemies` | ⬜ |
| AI (modular state machine) | `game::enemies` | ⬜ |
| Combat (damage, knockback, status) | `game::combat` | ⬜ |
| Health | `game::combat` | ⬜ |
| Corruption modifiers | `game::corruption` | ⬜ |
| Loot + resource economy | `game::inventory` | ⬜ |

## Phase 5 — World

| Subsystem | Crate | Status |
|---|---|---|
| Procedural terrain (layered noise) | `arcane_world::procedural` | ⬜ |
| Biome masks, vertical tiers | `arcane_world::procedural` | ⬜ |
| Caves / underground | `arcane_world::procedural` | ⬜ |
| Ley lines (spline graph) | `arcane_world::procedural` | ⬜ |
| Mana density field | `arcane_world::procedural` | ⬜ |
| Chunk streaming (async) | `arcane_world::streaming` | ⬜ |
| Persistence (chunk-level + global) | `arcane_world::save` | ⬜ |

## Phase 6 — Building and Research

| Subsystem | Crate | Status |
|---|---|---|
| Building (placement, stability, persistence) | `game::building` | ⬜ |
| Sanctuaries (storage, crafting, research, regen) | `game::sanctuaries` | ⬜ |
| Crafting | `game::sanctuaries` | ⬜ |
| Research tree (runes, schematics, upgrades) | `game::research` | ⬜ |
| Schematics (discovery, learning, reproduction) | `game::schematics` | ⬜ |
| Rune tablets (placeable runes) | `game::runes` | ⬜ |

## Phase 7 — Presentation

| Subsystem | Crate | Status |
|---|---|---|
| Lighting polish | `arcane_render::lighting` | ⬜ |
| Material system (magical intensity channel) | `arcane_render::materials` | ⬜ |
| VFX (pooled particles, trails, ribbons) | `arcane_vfx` | ⬜ |
| Audio (positional, ambient layers) | `arcane_audio` | ⬜ |
| Environmental atmosphere (fog, volumetrics) | `arcane_render::atmosphere` | ⬜ |
| UI (custom, rune-integrated) | `arcane_ui` | ⬜ |
| Animations | `arcane_render::animation` | ⬜ |
| World storytelling (ruins, murals) | `game::world` | ⬜ |

## Phase 8 — Optimization

| Concern | Status |
|---|---|
| CPU frame-time budget established | ⬜ |
| GPU frame-time budget established | ⬜ |
| Streaming latency profiled | ⬜ |
| Procedural-gen frame spikes eliminated | ⬜ |
| VFX pooling verified (zero per-frame alloc) | ⬜ |
| Asset load time profiled | ⬜ |

## Phase 9 — QA

| Area | Status |
|---|---|
| Functional test pass | ⬜ |
| Save/load long-session | ⬜ |
| World-gen determinism | ⬜ |
| Asset validation pass | ⬜ |
| Performance regression suite | ⬜ |

## Phase 10 — Release

| Deliverable | Status |
|---|---|
| Clean `main` | ⬜ |
| Reproducible build instructions | ⬜ |
| Attribution records complete | ⬜ |
| Documentation complete | ⬜ |
