#version 450

// Lit fragment shader.
//
// Lighting model: ambient + diffuse directional light.
//   out_color = ambient_base * albedo
//             + max(0, dot(N, L)) * sun_color * albedo
//
// Where N = normalized world-space normal, L = direction *toward* the
// light source (so dot(N, L) > 0 when the surface faces the light).
// The light direction is hard-coded here; in a future phase it will be
// delivered via a UBO so the game loop can animate it.

layout(location = 0) in vec3 v_color;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_world_pos;

layout(location = 0) out vec4 out_color;

void main() {
    vec3 albedo = v_color;

    // Light comes from the upper-front-right of the scene, slightly above
    // and to the +X side. This is a fixed "sun" direction in world space.
    vec3 light_dir = normalize(vec3(0.5, 0.8, 0.6));

    // Ambient term — keeps shadowed faces visible at low intensity so the
    // cube doesn't disappear into pure black on its dark side.
    vec3 ambient = vec3(0.15, 0.15, 0.18) * albedo;

    // Diffuse term — Lambertian: intensity = max(0, N · L).
    vec3 N = normalize(v_normal);
    float diff = max(dot(N, light_dir), 0.0);
    vec3 sun_color = vec3(1.0, 0.95, 0.85); // warm white
    vec3 diffuse = diff * sun_color * albedo;

    // Cheap rim term: highlight edges where N is nearly perpendicular to the
    // view direction. Adds a small back-light along silhouette edges so
    // the cube reads as a 3D object even on faces the light doesn't reach.
    // (Not a PBR specular; just enough to look better than flat shading.)
    vec3 view_dir = normalize(vec3(0.0, 0.0, 5.0) - v_world_pos);
    float rim = pow(1.0 - max(dot(N, view_dir), 0.0), 3.0) * 0.25;
    vec3 rim_color = vec3(0.6, 0.7, 1.0) * rim;

    out_color = vec4(ambient + diffuse + rim_color, 1.0);
}
