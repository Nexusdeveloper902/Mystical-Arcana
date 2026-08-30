# Mystical Arcana development environment
export MYSTICAL_ROOT=/home/z/my-project/mystical-arcana
export PATH="/home/z/.local/bin:/home/z/.cargo/bin:/home/z/my-project/tools/gdb/usr/bin:/home/z/my-project/tools/deps/usr/bin:$PATH"
export LD_LIBRARY_PATH="/home/z/my-project/tools/deps/usr/lib/x86_64-linux-gnu:/home/z/my-project/tools/deps/lib/x86_64-linux-gnu:/home/z/my-project/tools/deps/usr/lib:${LD_LIBRARY_PATH:-}"
# Static libshaderc.so.1 was shipped as dev-only — runtime is in libshaderc1 deb above.
export VK_ICD_FILENAMES=/home/z/my-project/tools/deps/usr/share/vulkan/icd.d/lvp_icd.json
export VK_LAYER_PATH=/home/z/my-project/tools/deps/usr/share/vulkan/explicit_layer.d
export VK_EXT_DEBUG_UTILS=1
export RUST_BACKTRACE=full
export VK_LOADER_DEBUG=warn
# For lavapipe headless surface
export VK_Icd_WSI=Headless
