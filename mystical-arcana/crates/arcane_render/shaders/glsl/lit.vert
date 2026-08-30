#version 450

// Lit vertex shader.
//
// Push constants carry both the view-projection matrix (64 bytes) and the
// per-instance model matrix (64 bytes), totalling 128 bytes — well within
// the Vulkan-guaranteed minimum push constant size of 128 bytes, and
// within the 256 bytes most desktop GPUs advertise.
//
// The vertex shader transforms position to clip space via view_proj * model
// and the world-space normal via mat3(model). For uniform-scale + rotation
// models (the case we care about) mat3(model) is also the correct normal
// matrix (inverse-transpose of a rotation == the rotation). Non-uniform
// scale would distort normals, but our scene uses uniform scale only.

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_color;
layout(location = 2) in vec3 in_normal;

layout(push_constant) uniform PC {
    mat4 view_proj; // 64 bytes — proj * view (camera)
    mat4 model;     // 64 bytes — per-instance world transform
} pc;

layout(location = 0) out vec3 v_color;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec3 v_world_pos;

void main() {
    vec4 world = pc.model * vec4(in_pos, 1.0);
    v_world_pos = world.xyz;
    v_color = in_color;
    v_normal = mat3(pc.model) * in_normal;
    gl_Position = pc.view_proj * world;
}
