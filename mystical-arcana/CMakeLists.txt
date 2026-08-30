# ===========================================================================
# Mystical Arcana — top-level CMake build
#
# Pure-Rust workspace + a CMake shell for asset baking / packaging.
# The actual game code lives in `crates/` and is built by Cargo. CMake just
# bridges to corrosion so a single `cmake --build` produces everything.
# ===========================================================================
cmake_minimum_required(VERSION 3.24)

project(MysticalArcana VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "" FORCE)
endif()

option(MYSTICAL_BUILD_TESTS "Build Rust tests via CMake" ON)

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

# ---------------------------------------------------------------------------
# Rust workspace crate exports
# ---------------------------------------------------------------------------
corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_SOURCE_DIR}/Cargo.toml"
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
        mystical_arcana
        asset_pipeline
)

# ---------------------------------------------------------------------------
# Asset baking target — calls asset_pipeline binary on the assets dir.
# This is a stub for now; the asset_pipeline binary is itself a stub.
# ---------------------------------------------------------------------------
add_custom_target(mystical_assets
    COMMAND asset_pipeline
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    COMMENT "Baking game assets"
)

# ---------------------------------------------------------------------------
# Install rules — copies the built binary into the staging directory.
# Note: corrosion_import_crate creates a target per Rust crate, named after
# the package name in the manifest. The game crate's package name is
# `mystical_arcana` (see crates/game/Cargo.toml).
# ---------------------------------------------------------------------------
install(IMPORTED_RUNTIME_ARTIFACTS mystical_arcana
    RUNTIME DESTINATION bin
)
