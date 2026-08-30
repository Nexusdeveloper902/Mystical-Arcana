# Mystical Arcana — Multi-Agent Worklog

This file is the single shared work log for all agents working on Mystical Arcana.
Each agent appends a new section after `---` when it finishes a task.

---
Task ID: 0-bootstrap
Agent: main (super-z)
Task: Bootstrap the development environment from a clean slate (no Rust toolchain, no CMake, no GDB, no Vulkan SDK were preinstalled).

Work Log:
- Verified previous session left no project files on disk (`/home/z/my-project` only had `download/`, `skills/`, `upload/`).
- Installed Rust 1.98.0 (stable) in user space via rustup to `~/.cargo/bin`.
- Installed CMake 4.4.2 and Ninja 1.13 via pip into `~/.local/bin` (apt was locked, no sudo).
- Downloaded `gdb 16.3-1` + all shared-library dependencies as `.deb` files and extracted them (no install) into `/home/z/my-project/tools/deps/`. GDB verified working.
- Downloaded and extracted `libvulkan-dev`, `mesa-vulkan-drivers`, `vulkan-tools`, `vulkan-validationlayers`, `glslang-tools`, `glslc`, `libshaderc-dev`.
- Confirmed the environment has **no physical GPU** (lspci empty). The `radv` driver that caused the original SIGSEGV in the previous session cannot work here.
- Confirmed **lavapipe** (CPU software Vulkan from Mesa) is the conformant path forward. `vulkaninfo --summary` reports GPU0 = `llvmpipe (LLVM 19.1.7, 256 bits)`, apiVersion 1.4.305, supports `VK_EXT_headless_surface`.
- Created `/home/z/my-project/scripts/env.sh` that sets PATH, LD_LIBRARY_PATH, VK_ICD_FILENAMES (lavapipe), VK_LAYER_PATH (Khronos validation), RUST_BACKTRACE=full, etc.

Stage Summary:
- Toolchain: cargo 1.98.0 / rustc 1.98.0 / cmake 4.4.2 / ninja 1.13 / gdb 16.3 / vulkaninfo 1.4.309.
- Vulkan driver: lavapipe (CPU). Validation layers available at `/home/z/my-project/tools/deps/usr/share/vulkan/explicit_layer.d/`.
- Source env script before any cargo/gdb/vulkan command: `source /home/z/my-project/scripts/env.sh`.
- The original SIGSEGV symptom ("radv is not a conformant Vulkan implementation" + SIGSEGV) was caused by attempting to load the AMD radeon ICD on a headless box with no AMD hardware. The fix path is to use lavapipe + `VK_EXT_headless_surface` for offscreen rendering to a framebuffer, then expose that framebuffer over HTTP for the browser "Render Observatory".
- The user explicitly forbade: disabling Vulkan, falling back to a null renderer, or faking GPU rendering. The plan honors this: real Vulkan via ash on lavapipe, real GPU command submission, real swapchain (headless), real shaders, real mesh buffers, real presentation to a host-visible image that the browser can fetch.
