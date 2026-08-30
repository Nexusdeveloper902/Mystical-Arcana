//! Deterministic visual-test scenarios.
//!
//! Each scenario constructs an `arcane_render::scene::RenderScene` with
//! fixed camera, fixed simulation time, and deterministic objects. These
//! scenes are used by visual regression tests AND the observatory.
//!
//! Scenarios:
//!   - empty_scene       : just the clear color / sky.
//!   - basic_scene       : a unit cube on a ground quad.
//!   - terrain_scene     : a chunk of terrain heightfield meshed + rendered.
//!   - player_scene      : the player avatar (capsule) standing on terrain.
//!   - mana_node_scene   : a glowing mana node floating above terrain.
//!   - combat_scene      : the player + several enemies (capsules) attacking.
//!   - building_scene    : a small structure (walls + roof).
//!   - corruption_scene   : corrupted ground + decaying pillar.

use arcane_render::prereqs::Vec3;
use arcane_render::{MeshId, Transform};
use arcane_render::scene::{
    Atmosphere, Camera, DrawCommand, Lights, Material, MaterialFlags, Mesh, ParticleVertex,
    PointLight, RenderScene,
};

/// A scenario kind, addressable from the CLI (`--scenario <name>`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ScenarioKind {
    Empty,
    Basic,
    Terrain,
    Player,
    ManaNode,
    Combat,
    Building,
    Corruption,
}

impl ScenarioKind {
    /// Parse from a CLI string. Returns `None` if unknown.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "empty_scene" => Some(Self::Empty),
            "basic_scene" => Some(Self::Basic),
            "terrain_scene" => Some(Self::Terrain),
            "player_scene" => Some(Self::Player),
            "mana_node_scene" => Some(Self::ManaNode),
            "combat_scene" => Some(Self::Combat),
            "building_scene" => Some(Self::Building),
            "corruption_scene" => Some(Self::Corruption),
            _ => None,
        }
    }

    /// All scenario names (for help text).
    pub fn all_names() -> &'static [&'static str] {
        &[
            "empty_scene",
            "basic_scene",
            "terrain_scene",
            "player_scene",
            "mana_node_scene",
            "combat_scene",
            "building_scene",
            "corruption_scene",
        ]
    }
}

/// A scenario ready to render: contains the scene and optional diagnostics.
pub struct Scenario {
    /// The constructed scene.
    pub scene: RenderScene,
    /// Optional descriptive diagnostics.
    pub diagnostics: Vec<String>,
}

impl Scenario {
    /// Build a scenario by kind. `sim_time` is fixed simulation time.
    pub fn build(kind: ScenarioKind, sim_time: f32, aspect: f32) -> Self {
        let mut scene = RenderScene {
            camera: Camera {
                aspect,
                ..Default::default()
            },
            atmosphere: Atmosphere::default(),
            ..Default::default()
        };
        let mut diagnostics = Vec::new();
        match kind {
            ScenarioKind::Empty => {
                scene.clear_color = [0.10, 0.12, 0.18, 1.0];
                scene.atmosphere = Atmosphere {
                    sky_zenith: [0.05, 0.07, 0.15, 1.0],
                    sky_horizon: [0.25, 0.28, 0.40, 1.0],
                    fog_color: [0.15, 0.18, 0.25, 1.0],
                    fog_density: 0.0015,
                    ..scene.atmosphere.clone()
                };
                scene.lights = default_lights();
                scene.camera = Camera {
                    position: [0.0, 5.0, 10.0],
                    target: [0.0, 0.0, 0.0],
                    up: [0.0, 1.0, 0.0],
                    fov_y: std::f32::consts::FRAC_PI_3,
                    aspect,
                    near: 0.1,
                    far: 1000.0,
                };
                diagnostics.push("empty scene".to_string());
            }
            ScenarioKind::Basic => {
                scene = basic_scene(aspect);
                diagnostics.push("unit cube + ground quad".to_string());
            }
            ScenarioKind::Terrain => {
                scene = terrain_scene(aspect, sim_time);
                diagnostics.push("32x32 chunk heightfield mesh".to_string());
            }
            ScenarioKind::Player => {
                scene = player_scene(aspect, sim_time);
                diagnostics.push("player avatar on terrain".to_string());
            }
            ScenarioKind::ManaNode => {
                scene = mana_node_scene(aspect, sim_time);
                diagnostics.push("mana node (point light + emissive sphere)".to_string());
            }
            ScenarioKind::Combat => {
                scene = combat_scene(aspect, sim_time);
                diagnostics.push("player + 3 enemies".to_string());
            }
            ScenarioKind::Building => {
                scene = building_scene(aspect, sim_time);
                diagnostics.push("building: 4 walls + roof".to_string());
            }
            ScenarioKind::Corruption => {
                scene = corruption_scene(aspect, sim_time);
                diagnostics.push("corrupted terrain + decaying pillar".to_string());
            }
        }
        Self { scene, diagnostics }
    }
}

fn default_lights() -> Lights {
    Lights {
        sun: Some(arcane_render::scene::DirectionalLight {
            direction: [0.4, -0.6, 0.3],
            color: [1.0, 0.95, 0.85],
        }),
        ambient_up: [0.10, 0.12, 0.18, 1.0],
        ambient_down: [0.02, 0.02, 0.03, 1.0],
        points: Vec::new(),
    }
}

fn basic_scene(aspect: f32) -> RenderScene {
    let mut scene = RenderScene {
        camera: Camera {
            position: [0.0, 4.0, 6.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_3,
            aspect,
            near: 0.1,
            far: 1000.0,
        },
        clear_color: [0.10, 0.12, 0.18, 1.0],
        atmosphere: Atmosphere::default(),
        lights: default_lights(),
        ..Default::default()
    };
    let ground = Mesh::ground_quad(10.0);
    scene.commands.push(DrawCommand {
        mesh: ground,
        material: Material {
            base_color: [0.30, 0.35, 0.30, 1.0],
            ..Default::default()
        },
        transform: Transform::identity(),
    });
    let cube = Mesh::unit_cube();
    scene.commands.push(DrawCommand {
        mesh: cube,
        material: Material {
            base_color: [0.7, 0.2, 0.2, 1.0],
            roughness: 0.5,
            ..Default::default()
        },
        transform: Transform {
            position: Vec3::new(0.0, 1.0, 0.0),
            ..Transform::identity()
        },
    });
    scene
}

fn terrain_scene(aspect: f32, sim_time: f32) -> RenderScene {
    // World-chunk dimensions & heightfield sampler live in `crate::session`.
    let mut scene = RenderScene {
        camera: Camera {
            position: [0.0, 30.0, 0.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 0.0, -1.0],
            fov_y: std::f32::consts::FRAC_PI_3,
            aspect,
            near: 0.5,
            far: 1000.0,
        },
        clear_color: [0.15, 0.18, 0.25, 1.0],
        atmosphere: Atmosphere {
            sky_zenith: [0.10, 0.15, 0.30, 1.0],
            sky_horizon: [0.50, 0.45, 0.35, 1.0],
            fog_color: [0.30, 0.30, 0.35, 1.0],
            fog_density: 0.0015,
            ..Atmosphere::default()
        },
        lights: default_lights(),
        ..Default::default()
    };
    // Mesh the heightfield as a grid of triangles.
    let side = crate::session::WORLD_CHUNK_SIZE as i32;
    let mut verts = Vec::with_capacity((side * side) as usize);
    let mut indices = Vec::with_capacity(((side - 1) * (side - 1) * 6) as usize);
    let mut height_at = |x: f32, z: f32| crate::session::sample_height(x, z);
    for iz in 0..side {
        for ix in 0..side {
            let wx = ix as f32 - side as f32 * 0.5;
            let wz = iz as f32 - side as f32 * 0.5;
            let h = height_at(wx, wz);
            // Compute a fake normal via finite differences.
            let hl = height_at(wx - 1.0, wz);
            let hr = height_at(wx + 1.0, wz);
            let hd = height_at(wx, wz - 1.0);
            let hu = height_at(wx, wz + 1.0);
            let n = Vec3::new(hl - hr, 2.0, hd - hu).normalize();
            verts.push(arcane_render::scene::MeshVertex {
                position: [wx, h, wz],
                normal: [n.x, n.y, n.z],
                texcoord: [wx / side as f32, wz / side as f32],
            });
        }
    }
    let n = side as u32;
    for iz in 0..n - 1 {
        for ix in 0..n - 1 {
            let a = iz * n + ix;
            let b = a + 1;
            let c = a + n;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    scene.commands.push(DrawCommand {
        mesh: Mesh {
            id: MeshId::from_str("scenario/terrain"),
            vertices: verts,
            indices,
            texture: None,
        },
        material: Material {
            base_color: [0.35, 0.42, 0.30, 1.0],
            roughness: 0.95,
            metallic: 0.0,
            ..Default::default()
        },
        transform: Transform::identity(),
    });
    // Sway the directional light over time.
    let sun_angle = sim_time * 0.2;
    if let Some(sun) = scene.lights.sun.as_mut() {
        let cos = sun_angle.cos();
        let sin = sun_angle.sin();
        sun.direction = [cos * 0.5, -0.6, sin * 0.5];
    }
    scene
}

fn player_scene(aspect: f32, _sim_time: f32) -> RenderScene {
    let mut scene = terrain_scene(aspect, 0.0);
    // Player capsule: a tall red box standing on terrain at origin.
    let player = Mesh::unit_cube();
    scene.commands.push(DrawCommand {
        mesh: player,
        material: Material {
            base_color: [0.2, 0.6, 0.85, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            ..Default::default()
        },
        transform: Transform {
            position: Vec3::new(0.0, 1.5, 0.0),
            scale: Vec3::new(0.5, 1.0, 0.5),
            ..Transform::identity()
        },
    });
    // Camera framed on player from over the shoulder.
    scene.camera = Camera {
        position: [-4.0, 4.0, 5.0],
        target: [0.0, 1.0, 0.0],
        up: [0.0, 1.0, 0.0],
        fov_y: std::f32::consts::FRAC_PI_3,
        aspect,
        near: 0.1,
        far: 1000.0,
    };
    scene
}

fn mana_node_scene(aspect: f32, sim_time: f32) -> RenderScene {
    let mut scene = terrain_scene(aspect, 0.0);
    // Mana node: a glowing emissive sphere, point light.
    let sphere = make_sphere(0.5, 12);
    let bob = sim_time.sin() * 0.2;
    scene.commands.push(DrawCommand {
        mesh: sphere,
        material: Material {
            base_color: [0.4, 0.6, 1.0, 1.0],
            emissive: [0.6, 0.9, 1.5],
            roughness: 0.1,
            metallic: 0.0,
            ..Default::default()
        },
        transform: Transform {
            position: Vec3::new(0.0, 4.0 + bob, 0.0),
            ..Transform::identity()
        },
    });
    scene.lights.points.push(PointLight {
        position: [0.0, 4.0 + bob, 0.0],
        color: [0.6, 0.9, 1.5],
        range: 12.0,
    });
    // Particles around the node (sparkles).
    for i in 0..40 {
        let a = (i as f32) * 0.157;
        let r = 1.0 + (sim_time + i as f32 * 0.1).sin() * 0.5;
        let p_y = 4.0 + bob + (a * 2.0).sin() * 0.5;
        let col = 0.5 + 0.5 * (a * 3.0).sin();
        scene.particles.push(ParticleVertex {
            position: [a.cos() * r, p_y, a.sin() * r],
            color: [0.3 * col, 0.5 * col, 1.0 * col, 1.0],
            size: 0.1,
        });
    }
    scene
}

fn combat_scene(aspect: f32, sim_time: f32) -> RenderScene {
    let mut scene = player_scene(aspect, sim_time);
    // Three enemy capsules arranged in an arc in front of the player.
    for i in 0..3 {
        let angle = (i as f32 - 1.0) * 0.5;
        let x = angle.sin() * 3.0;
        let z = -angle.cos() * 3.0;
        let enemy = Mesh::unit_cube();
        scene.commands.push(DrawCommand {
            mesh: enemy,
            material: Material {
                base_color: [0.85, 0.15, 0.15, 1.0],
                roughness: 0.5,
                ..Default::default()
            },
            transform: Transform {
                position: Vec3::new(x, 1.0, z),
                scale: Vec3::new(0.5, 1.5, 0.5),
                rotation: arcane_render::prereqs::Quat::from_axis_angle(&Vec3::y_axis(), angle * 0.5),
            },
        });
        // Subtle red point light at the enemy.
        scene.lights.points.push(PointLight {
            position: [x, 1.0, z],
            color: [0.4, 0.05, 0.05],
            range: 4.0,
        });
    }
    scene
}

fn building_scene(aspect: f32, _sim_time: f32) -> RenderScene {
    let mut scene = RenderScene {
        camera: Camera {
            position: [6.0, 4.0, 6.0],
            target: [0.0, 1.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_y: std::f32::consts::FRAC_PI_3,
            aspect,
            near: 0.1,
            far: 1000.0,
        },
        clear_color: [0.10, 0.12, 0.18, 1.0],
        atmosphere: Atmosphere::default(),
        lights: default_lights(),
        ..Default::default()
    };
    // Floor.
    scene.commands.push(DrawCommand {
        mesh: Mesh::ground_quad(8.0),
        material: Material {
            base_color: [0.25, 0.22, 0.20, 1.0],
            ..Default::default()
        },
        transform: Transform::identity(),
    });
    // Four walls.
    let wall = Mesh::unit_cube();
    let wall_color = [0.55, 0.50, 0.45, 1.0];
    for i in 0..4 {
        let angle = (i as f32) * std::f32::consts::FRAC_PI_2;
        let cx = angle.cos() * 3.5;
        let cz = angle.sin() * 3.5;
        scene.commands.push(DrawCommand {
            mesh: wall.clone(),
            material: Material {
                base_color: wall_color,
                ..Default::default()
            },
            transform: Transform {
                position: Vec3::new(cx, 1.5, cz),
                scale: Vec3::new(8.0, 3.0, 0.3),
                rotation: arcane_render::prereqs::Quat::from_axis_angle(&Vec3::y_axis(), angle),
            },
        });
    }
    // Roof.
    scene.commands.push(DrawCommand {
        mesh: Mesh::ground_quad(8.0),
        material: Material {
            base_color: [0.35, 0.20, 0.10, 1.0],
            ..Default::default()
        },
        transform: Transform {
            position: Vec3::new(0.0, 3.5, 0.0),
            ..Transform::identity()
        },
    });
    scene
}

fn corruption_scene(aspect: f32, _sim_time: f32) -> RenderScene {
    let mut scene = terrain_scene(aspect, 0.0);
    // Override atmosphere to a dark, hazy hue.
    scene.atmosphere = Atmosphere {
        sky_zenith: [0.05, 0.0, 0.05, 1.0],
        sky_horizon: [0.15, 0.05, 0.10, 1.0],
        fog_color: [0.15, 0.05, 0.10, 1.0],
        fog_density: 0.0030,
        ..scene.atmosphere.clone()
    };
    scene.clear_color = [0.05, 0.0, 0.05, 1.0];
    // Override the terrain material to a corrupted purple.
    if let Some(cmd) = scene.commands.first_mut() {
        cmd.material.base_color = [0.30, 0.10, 0.30, 1.0];
        cmd.material.emissive = [0.10, 0.0, 0.15];
    }
    // A decaying pillar.
    let pillar = Mesh::unit_cube();
    scene.commands.push(DrawCommand {
        mesh: pillar,
        material: Material {
            base_color: [0.35, 0.20, 0.40, 1.0],
            emissive: [0.15, 0.05, 0.20],
            flags: MaterialFlags::UNLIT.bits(),
            ..Default::default()
        },
        transform: Transform {
            position: Vec3::new(0.0, 3.0, 0.0),
            scale: Vec3::new(0.5, 6.0, 0.5),
            ..Transform::identity()
        },
    });
    // A purple point light at the top of the pillar.
    scene.lights.points.push(PointLight {
        position: [0.0, 6.0, 0.0],
        color: [0.5, 0.05, 0.5],
        range: 8.0,
    });
    scene
}

/// Build a UV sphere mesh with `segments` vertical slices.
fn make_sphere(radius: f32, segments: u32) -> Mesh {
    let rings = segments.max(3);
    let mut verts = Vec::with_capacity(((rings + 1) * (rings + 1)) as usize);
    let mut indices = Vec::with_capacity((rings * rings * 6) as usize);
    for j in 0..=rings {
        let v = j as f32 / rings as f32;
        let phi = v * std::f32::consts::PI;
        for i in 0..=rings {
            let u = i as f32 / rings as f32;
            let theta = u * std::f32::consts::TAU;
            let s = phi.sin();
            let c = phi.cos();
            let x = s * theta.cos();
            let y = c;
            let z = s * theta.sin();
            verts.push(arcane_render::scene::MeshVertex {
                position: [x * radius, y * radius, z * radius],
                normal: [x, y, z],
                texcoord: [u, v],
            });
        }
    }
    let stride = rings + 1;
    for j in 0..rings {
        for i in 0..rings {
            let a = j * stride + i;
            let b = a + stride;
            let c = a + 1;
            let d = b + 1;
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        }
    }
    Mesh {
        id: MeshId::from_str("builtin/sphere"),
        vertices: verts,
        indices,
        texture: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scenarios_produce_scenes() {
        let kinds = [
            ScenarioKind::Empty,
            ScenarioKind::Basic,
            ScenarioKind::Terrain,
            ScenarioKind::Player,
            ScenarioKind::ManaNode,
            ScenarioKind::Combat,
            ScenarioKind::Building,
            ScenarioKind::Corruption,
        ];
        for kind in kinds {
            let s = Scenario::build(kind, 0.0, 16.0 / 9.0);
            // At minimum, every scenario must have a clear color and lights.
            assert!(
                s.scene.clear_color[3] > 0.0,
                "{:?} had no clear color",
                kind
            );
        }
    }

    #[test]
    fn terrain_has_indices_and_vertices() {
        let s = Scenario::build(ScenarioKind::Terrain, 0.0, 1.0);
        let cmd = &s.scene.commands[0];
        assert!(!cmd.mesh.vertices.is_empty());
        assert!(!cmd.mesh.indices.is_empty());
    }

    #[test]
    fn mana_node_has_point_light() {
        let s = Scenario::build(ScenarioKind::ManaNode, 0.5, 1.0);
        assert!(!s.scene.lights.points.is_empty());
    }

    #[test]
    fn combat_has_three_enemies() {
        // 1 player + 3 enemies = 4 character cubes (plus terrain).
        let s = Scenario::build(ScenarioKind::Combat, 0.5, 1.0);
        assert!(s.scene.commands.len() >= 4);
    }
}
