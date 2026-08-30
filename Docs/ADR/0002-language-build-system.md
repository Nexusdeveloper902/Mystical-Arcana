# ADR-0002: Language, build system, and Rust↔CMake bridge

**Date:** 2026-08-30
**Status:** Accepted

## Context

The brief requires a custom Vulkan-based engine. Section 7 left language choice open; the user later directed the choice explicitly.

## Decision

- **Runtime language:** Rust (stable, currently 1.98).
- **Build system:** CMake + [Corrosion](https://github.com/corrosion-rs/corrosion) v0.5.
- **Vulkan bindings:** [`ash`](https://crates.io/crates/ash) — zero-overhead, hand-written.
- **Windowing:** `winit`.
- **Math:** `nalgebra` (with engine-side SIMD-friendly wrappers in `arcane_math`).
- **Physics:** `rapier3d`.
- **Audio:** `kira`.
- **Serialization:** `serde` + `ron` (data defs) + `postcard` (saves).

## Rationale

- **Rust** — Memory safety without GC; no per-frame allocations in hot paths; `cargo` is fast and reproducible; `ash` is the de-facto Rust Vulkan binding; ecosystem crates for glTF, audio, physics are mature.
- **CMake + Corrosion** — CMake is the industry standard build system and the user's explicit choice. Corrosion lets CMake drive the Rust workspace so we can later add native C++ tools or asset pipeline stages alongside Rust crates.
- **`ash` over `wgpu`/`rend3`** — The brief explicitly forbids embedding another engine underneath. `ash` is a thin binding layer, not an engine; `wgpu` is a higher-level abstraction that would hide too much of the rendering architecture we need to control for *Mystical Arcana*'s stylized magical identity.

## Consequences

- The whole engine compiles with `cargo`. CMake is a thin orchestration layer today; it pays off once we add C++ asset tools or platform-specific native libraries.
- `panic = "abort"` in release — keeps binary size small and signal handling predictable. Tests use the default panic handler.
- `nalgebra` over a hand-rolled math library: we will not re-implement matrix/quaternion algebra when `nalgebra` is mature and SIMD-tuned. Custom math wrappers in `arcane_math` provide cache-friendly AoS vec storage where needed.

## Alternatives considered

- *C++ throughout* — Industry standard, more reference code, but no memory safety guarantees and longer compile times. Rejected per user direction.
- *Rust + `wgpu`* — Rejected as too high-level; violates the spirit of "build a custom engine."
- *Pure Cargo, no CMake* — Simpler, but the user explicitly asked for CMake + Corrosion.
