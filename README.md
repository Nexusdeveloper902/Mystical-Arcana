# Mystical Arcana

A first-person systemic fantasy survival, exploration, crafting, and magic game built on **Arcane** — a purpose-built, Vulkan-based engine designed specifically for it.

> **The defining idea:** *Mystical Arcana is a game about replacing tools with understanding.*

The player is an Arcanist — once a craftsman whose body developed an irreversible condition causing ordinary instruments to decay when touched. Forced to abandon conventional technology, they turn to the forgotten Arcanum. Their replacement for tools is **will**. Their medium is **mana**. Their language is **runes**. Their laboratory is **the world**.

## Status

Active development. See [`Docs/ADR/`](Docs/ADR/) for architecture decisions and [`Docs/development-roadmap.md`](Docs/development-roadmap.md) for the phased plan.

## Technology stack — the **Arcane** engine

| Concern | Choice |
|---|---|
| Engine runtime language | Rust (stable) |
| Build system | CMake + [Corrosion](https://github.com/corrosion-rs/corrosion) |
| Graphics API | Vulkan via [`ash`](https://crates.io/crates/ash) |
| Windowing | `winit` |
| Math | `nalgebra` (with engine-side wrappers for cache-friendliness) |
| Physics | `rapier3d` |
| Audio | `kira` |
| Asset interchange | glTF 2.0, PNG, OGG |
| Serialization | `serde` (RON for data defs, postcard for saves) |
| Procedural generation | custom deterministic noise + layered generator |
| Testing | `cargo test`, headless integration harness |

The core runtime architecture belongs to the **Arcane** engine. Third-party crates are used only for individual capabilities, never embedded as a complete engine underneath. All engine-facing types live under the `arcane_*` crate family.

## Repository layout

```
MysticalArcana/
├── CMakeLists.txt                  # Top-level CMake entry; pulls Corrosion
├── Cargo.toml                      # Rust workspace root
├── corrosion/                      # Vendored Corrosion (CMake<->Cargo bridge)
├── crates/
│   ├── arcane_core/                # Math, logging, memory, threading, ECS, profiling
│   ├── arcane_render/              # Arcane Renderer - abstraction + Vulkan backend (ash)
│   ├── arcane_world/               # Arcane World - chunks, streaming, procedural gen, saves
│   ├── arcane_assets/              # Arcane Asset Pipeline - cooker, validator
│   ├── arcane_audio/               # Arcane Audio
│   ├── arcane_physics/             # Arcane Physics (rapier3d)
│   ├── arcane_input/               # Arcane Input - abstraction, action mapping, rebinding
│   ├── arcane_ui/                  # Arcane UI - custom UI system, rune-aware
│   ├── arcane_vfx/                 # Arcane VFX - pooled particle/VFX system
│   └── game/                       # Mystical Arcana game logic + main binary
├── Tools/
│   └── asset_pipeline/             # Standalone asset cooking/validation tools
├── Tests/                          # Integration tests, headless smoke tests
├── Assets/
│   ├── shaders/                    # GLSL -> SPIR-V source
│   ├── data/                       # Rune defs, spell defs, biome defs, item defs
│   ├── textures/  models/  audio/
├── Docs/
│   ├── ADR/                        # Architecture Decision Records
│   ├── design.md                   # Creative direction (canonical)
│   ├── build-instructions.md
│   └── attribution.md              # Third-party asset attribution
└── Build/                          # Build artifacts (gitignored)
```

## Build

```bash
# From the repository root
cmake -B build -S .
cmake --build build --config Release
```

The CMake build drives the Rust workspace via Corrosion. Tests run via `cargo test`:

```bash
cargo test --workspace --all-features
```

A headless integration harness can run without a GPU:

```bash
cargo run --bin mystical_arcana --features headless -- --headless --smoke
```

See [`Docs/build-instructions.md`](Docs/build-instructions.md) for platform-specific setup.

## Branching

- `main` — stable, always-buildable
- `develop` — active integration
- `feature/<system>` — major subsystem work
- `qa/<area>` — quality assurance validation

## License

TBD — see [`LICENSE`](LICENSE) once finalized. Third-party asset attribution is recorded in [`Docs/attribution.md`](Docs/attribution.md).
