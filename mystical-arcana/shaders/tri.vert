#version 450 core

// Inputs from vertex buffer
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_color;

// Output to fragment shader
layout(location = 0) out vec3 v_color;

// Push constant: view-projection matrix
layout(push_constant) uniform PC {
    mat4 u_mvp;
} pc;

void main() {
    v_color = a_color;
    gl_Position = pc.u_mvp * vec4(a_pos, 1.0);
}
