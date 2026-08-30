# ADR-0003: Headless-first testing & validation strategy

**Date:** 2026-08-30
**Status:** Accepted

## Context

The development environment may not have a Vulkan-capable GPU or display. The brief requires "a testing and validation layer that can test absolutely everything even in a no GPU environment." Several engine subsystems (Vulkan surface, shader pipeline output, audio device, input hardware) are inherently hardware-bound — we cannot run them meaningfully in a headless sandbox.

## Decision

Every engine and game subsystem must be **separable into a pure-logic core and a thin hardware surface**. The pure-logic core is fully unit-testable headless. The hardware surface is feature-gated behind `--features headless|gpu`.

| System | Pure-logic (headless-testable) | Hardware surface (feature-gated) |
|---|---|---|
| Mana, runes, spells, schematics, inventory, combat, AI | ✅ all logic | — |
| Procedural world: noise, biomes, ley lines, mana density, caves | ✅ deterministic | — |
| Save system: versioned binary, roundtrip | ✅ fully | — |
| ECS, transforms, math | ✅ fully | — |
| Asset pipeline: validation, cooking | ✅ CPU-only | optional GPU mesh processing |
| Vulkan renderer | device selection logic, descriptor layouts, pipeline state objects | instance, surface, swapchain |
| Audio | parameter curves, layer mixing rules | audio device playback |
| Input | action mapping, rebinding logic, deadzone math | raw device polling |
| Player controller | movement integration, collision query interface | — |

The integration test harness in `Tests/` runs `cargo test --workspace --all-features` and a `--headless` binary smoke test that exercises the gameplay loop without presenting a window.

## Consequences

- Every crate exposes a `Headless` test surface where pure logic lives.
- Hardware-bound code is isolated behind traits (`Renderer`, `AudioBackend`, `InputSource`) so the test harness can substitute no-op implementations.
- The smoke test in `Tests/smoke_headless.rs` runs the player through a script: walk → find Mana Node → gain mana → cast Gather Bolt → collect resource → inventory → save → load → assert state matches.
- Procedural generation determinism is enforced: `seed == N → identical chunk output across platforms`.

## Alternatives considered

- *Lavapipe (software Vulkan)* — Too slow for the smoke test; reasonable for shader compile-validation only. Available as a fallback path when `MA_HEADLESS=ON`.
- *CI-only GPU tests* — Possible but impractical in this environment. Documented for future CI setup.
