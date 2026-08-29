cmake_minimum_required(VERSION 3.22)

# Project version synced with Cargo workspace
project(MysticalArcana
    VERSION 0.1.0
    LANGUAGES CXX
    DESCRIPTION "Mystical Arcana — first-person systemic fantasy survival built on the Arcane engine"
)

# ---------------------------------------------------------------------------
# Build configuration
# ---------------------------------------------------------------------------
option(MA_BUILD_TESTS "Build integration tests" ON)
option(MA_HEADLESS "Headless build (no Vulkan surface, no window)" OFF)
option(MA_ENABLE_VULKAN_VALIDATION "Enable Vulkan validation layers at runtime" OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# ---------------------------------------------------------------------------
# Corrosion — Rust <-> CMake bridge
# ---------------------------------------------------------------------------
include(FetchContent)
FetchContent_Declare(
    corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.5.0
)
FetchContent_MakeAvailable(corrosion)

# Tell Corrosion where our workspace lives.
corrosion_configure_cargo(
    CARGO_MANIFEST_DIR "${CMAKE_SOURCE_DIR}"
)

# ---------------------------------------------------------------------------
# Rust workspace crate exports
# ---------------------------------------------------------------------------
# Each Rust crate is built by Cargo (via Corrosion) and surfaced as a CMake
# target named `Arcane::<crate>` so that native C++ tools/tests in the future
# can link against them. The Rust side does the real work today.
corrosion_import_crate(MANIFEST_PATH "${CMAKE_SOURCE_DIR}/Cargo.toml"
    CRATES
        arcane_core
        arcane_math
        arcane_ecs
        arcane_render
        arcane_world
        arcane_assets
        arcane_audio
        arcane_physics
        arcane_input
        arcane_ui
        arcane_vfx
        game
        asset_pipeline
)

# Top-level convenience target: build the entire Rust workspace + run all tests.
add_custom_target(Arcane.All
    COMMAND ${CMAKE_COMMAND} -E cmake_echo_color --cyan -- "Building Arcane engine + game via Cargo..."
    DEPENDS game arcane_render arcane_world arcane_assets arcane_audio arcane_physics arcane_input arcane_ui arcane_vfx
    COMMENT "Assembling the Arcane engine and Mystical Arcana game"
)

add_custom_target(Arcane.Tests
    COMMAND cargo test --workspace --all-features
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    COMMENT "Running the Arcane + Mystical Arcana test suite (headless-safe)"
)

add_custom_target(Arcane.Run
    COMMAND cargo run --release --bin mystical_arcana -- $<$<BOOL:${MA_HEADLESS}>:--headless>
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    COMMENT "Launching Mystical Arcana"
)

# ---------------------------------------------------------------------------
# Documentation / install hooks (kept minimal)
# ---------------------------------------------------------------------------
install(FILES README.md DESTINATION share/mystical_arcana)
