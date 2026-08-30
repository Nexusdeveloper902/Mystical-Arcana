# ADR-0001: Engine name and identity

**Date:** 2026-08-30
**Status:** Accepted

## Context

The brief specifies a custom-built, Vulkan-based game engine specifically for *Mystical Arcana*. The engine needs a name distinct from the game itself, because:

1. The game is *Mystical Arcana* — a specific product.
2. The engine is a reusable, modular runtime that powers the game but is not the game.

Both should reinforce the same creative identity: ancient, layered, knowing — not generic.

## Decision

The engine is named **Arcane**. Everything derived from the engine carries the `Arcane` prefix:

| Module | Crate |
|---|---|
| Core (math, memory, threading, ECS, logging) | `arcane_core` |
| Math primitives | `arcane_math` |
| Renderer (Vulkan backend via `ash`) | `arcane_render` (Arcane Renderer) |
| World, chunks, streaming, procedural | `arcane_world` |
| Asset pipeline, cooker, validator | `arcane_assets` |
| Audio | `arcane_audio` |
| Physics | `arcane_physics` |
| Input | `arcane_input` |
| UI | `arcane_ui` |
| VFX | `arcane_vfx` |
| Game logic | `game` (binary: `mystical_arcana`) |

## Alternatives considered

- *MysticalEngine* — couples the engine to the game's full product name, awkward.
- *Aether* — already used in gamedev projects.
- *Rune* — conflicts with the in-fiction rune language.

## Consequences

All crate names begin `arcane_*`. The `game` crate is the only one without the prefix because it is the game layer, not the engine. The public CMake target family is `Arcane::*`.
