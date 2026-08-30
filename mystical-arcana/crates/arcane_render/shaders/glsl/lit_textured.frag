#version 450

// Phase F fragment shader.
//
// Lighting model: ambient + diffuse directional light + rim, with the
// albedo modulated by a procedural checker pattern generated directly
// in the shader (no texture fetch needed).
//
// Why procedural instead of a sampled texture? lavapipe (Mesa's CPU
// Vulkan) segfaults inside create_graphics_pipelines when the fragment
// shader actually calls texture(sampler2D, vec2) — the descriptor set
// + sampler are bound correctly, but the NIR→LLVM lowering of the
// texture fetch fails. The descriptor set layout, pool, set, texture
// image, and sampler are all still wired through the pipeline (kept
// for a future host with a conformant GPU); the procedural checker
// just gives us the visual effect of a tiled ground texture on
// lavapipe without triggering the bug.

layout(location = 0) in vec3 v_color;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_world_pos;
layout(location = 3) in vec2 v_uv;

layout(location = 0) out vec4 out_color;

// 8x8 checker pattern in UV space, alternating between two warm colors
// so it's visible against the cool background clear.
vec3 checker(vec2 uv) {
    vec2 cell = floor(uv * 8.0);
    float v = mod(cell.x + cell.y, 2.0);
    vec3 color_a = vec3(0.86, 0.78, 0.62); // warm light tan
    vec3 color_b = vec3(0.47, 0.31, 0.23); // warm dark brown
    return mix(color_a, color_b, v);
}

void main() {
    vec3 tex_color = checker(v_uv);
    vec3 albedo = v_color * tex_color;

    vec3 light_dir = normalize(vec3(0.5, 0.8, 0.6));
    vec3 ambient = vec3(0.15, 0.15, 0.18) * albedo;
    vec3 N = normalize(v_normal);
    float diff = max(dot(N, light_dir), 0.0);
    vec3 sun_color = vec3(1.0, 0.95, 0.85);
    vec3 diffuse = diff * sun_color * albedo;
    vec3 view_dir = normalize(vec3(0.0, 0.0, 5.0) - v_world_pos);
    float rim = pow(1.0 - max(dot(N, view_dir), 0.0), 3.0) * 0.25;
    vec3 rim_color = vec3(0.6, 0.7, 1.0) * rim;

    out_color = vec4(ambient + diffuse + rim_color, 1.0);
}
