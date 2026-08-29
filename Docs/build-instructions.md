# Build Instructions

## Prerequisites

| Tool | Minimum version | Notes |
|---|---|---|
| Rust | 1.75 stable | Install via https://rustup.rs |
| CMake | 3.22 | Used only as a thin orchestration layer |
| Vulkan loader | 1.3+ | `libvulkan.so.1` (Linux) / `vulkan-1.dll` (Windows) / Vulkan SDK (macOS/MoltenVK) |
| C++ compiler | C++17 | Only required if you add native asset-pipeline stages later |

On Debian/Ubuntu: `sudo apt install cmake libvulkan-dev glslang-tools`
On Arch/Manjaro: `sudo pacman -S cmake vulkan-devel glslang`
On Windows: install Vulkan SDK from https://vulkan.lunarg.com/
On macOS: `brew install cmake molten-vulkan`

## Build

```bash
# Build everything via CMake (drives Cargo via Corrosion)
cmake -B build -S .
cmake --build build --config Release

# Or, build directly via Cargo (faster for Rust-only iteration)
cargo build --workspace --release
```

## Run

```bash
# Run the game (requires a Vulkan-capable GPU + display)
cargo run --release --bin mystical_arcana

# Run the headless smoke test (no GPU needed; for CI)
cargo run --release --bin mystical_arcana --features headless -- --headless --smoke
```

## Test

```bash
# All unit + integration tests across the workspace
cargo test --workspace --all-features

# Run only the procedural-generation determinism tests
cargo test -p arcane_world --test procedural_determinism

# Run only the save/load roundtrip tests
cargo test -p arcane_world --test save_roundtrip

# Run the headless gameplay loop smoke test
cargo test --test smoke_headless
```

## Performance benchmarks

```bash
cargo bench --workspace
```

Results land in `target/criterion/`.

## Vulkan validation

```bash
# Enable Vulkan validation layers at runtime (debug builds only)
cmake -B build -S . -DMA_ENABLE_VULKAN_VALIDATION=ON
# or
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo run
```

## Reproducibility

- `Cargo.lock` is committed for reproducible builds.
- `rust-toolchain.toml` pins the toolchain (added in a later commit).
- Build options are exposed in `CMakePresets.json` once added.
