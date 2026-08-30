#version 450

// Phase F vertex shader.
//
// Same push-constant layout as Phase E (view_proj + model = 128 bytes).
// Passes UV (location 3) to the fragment shader so the procedural
// checker pattern can be evaluated per-fragment.

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_color;
layout(location = 2) in vec3 in_normal;
layout(location = 3) in vec2 in_uv;

layout(push_constant) uniform PC {
    mat4 view_proj;
    mat4 model;
} pc;

layout(location = 0) out vec3 v_color;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec3 v_world_pos;
layout(location = 3) out vec2 v_uv;

void main() {
    vec4 world = pc.model * vec4(in_pos, 1.0);
    v_world_pos = world.xyz;
    v_color = in_color;
    v_normal = mat3(pc.model) * in_normal;
    v_uv = in_uv;
    gl_Position = pc.view_proj * world;
}
