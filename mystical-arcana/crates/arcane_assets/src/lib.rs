//! arcane_assets — asset loading and packaging.
//!
//! Phase J adds a minimal Wavefront OBJ parser. Supports:
//!   * `v x y z` — vertex position
//!   * `vn x y z` — vertex normal (optional; if absent, generates face normals)
//!   * `f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3 ...` — face (triangulates n>3)
//!
//! Doesn't support: materials, textures, groups, smoothing, comments
//! mid-line, negative indices. Enough for a simple embedded OBJ asset.
//!
//! Also exposes a few embedded test assets (octahedron) so the engine
//! can demo "asset loading" without a real disk asset pipeline.

/// A parsed OBJ model: vertex positions, vertex normals (one per position),
/// and triangle indices into the position array.
#[derive(Debug, Clone)]
pub struct ObjModel {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u16>,
}

impl ObjModel {
    /// Total vertex count (== positions.len() == normals.len()).
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Total triangle count (== indices.len() / 3).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Parse a Wavefront OBJ string into an [`ObjModel`]. Returns an error
/// message on the first parse failure (line number + reason).
///
/// Faces are triangulated via fan triangulation (vertex 0 + consecutive
/// pairs). This works for convex polygons; concave polygons would need
/// ear-clipping which is more complex than we need here.
pub fn parse_obj(source: &str) -> Result<ObjModel, String> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut faces: Vec<Vec<ObjFaceIndex>> = Vec::new();

    for (line_no, raw) in source.lines().enumerate() {
        // Strip comments.
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let kind = tokens.next().ok_or_else(|| {
            format!("line {}: empty after comment strip", line_no + 1)
        })?;
        match kind {
            "v" => {
                let nums: Vec<&str> = tokens.collect();
                if nums.len() < 3 {
                    return Err(format!(
                        "line {}: 'v' needs 3 floats, got {}",
                        line_no + 1,
                        nums.len()
                    ));
                }
                let x: f32 = nums[0].parse().map_err(|e| {
                    format!("line {}: v.x parse error: {}", line_no + 1, e)
                })?;
                let y: f32 = nums[1].parse().map_err(|e| {
                    format!("line {}: v.y parse error: {}", line_no + 1, e)
                })?;
                let z: f32 = nums[2].parse().map_err(|e| {
                    format!("line {}: v.z parse error: {}", line_no + 1, e)
                })?;
                positions.push([x, y, z]);
            }
            "vn" => {
                let nums: Vec<&str> = tokens.collect();
                if nums.len() < 3 {
                    return Err(format!(
                        "line {}: 'vn' needs 3 floats, got {}",
                        line_no + 1,
                        nums.len()
                    ));
                }
                let x: f32 = nums[0].parse().map_err(|e| {
                    format!("line {}: vn.x parse error: {}", line_no + 1, e)
                })?;
                let y: f32 = nums[1].parse().map_err(|e| {
                    format!("line {}: vn.y parse error: {}", line_no + 1, e)
                })?;
                let z: f32 = nums[2].parse().map_err(|e| {
                    format!("line {}: vn.z parse error: {}", line_no + 1, e)
                })?;
                normals.push([x, y, z]);
            }
            "f" => {
                let verts: Vec<ObjFaceIndex> = tokens
                    .map(|tok| parse_face_vertex(tok))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("line {}: face vertex parse: {}", line_no + 1, e))?;
                if verts.len() < 3 {
                    return Err(format!(
                        "line {}: face needs >= 3 vertices, got {}",
                        line_no + 1,
                        verts.len()
                    ));
                }
                faces.push(verts);
            }
            // Skip unsupported directives (vt, g, mtllib, usemtl, s, o, etc).
            _ => {}
        }
    }

    // Build a unified vertex array: for each face, we triangulate via
    // fan (vertex 0, i, i+1). If the face provides normals, we use
    // them; otherwise we synthesize face normals from the positions.
    let mut out_positions: Vec<[f32; 3]> = Vec::new();
    let mut out_normals: Vec<[f32; 3]> = Vec::new();
    let mut out_indices: Vec<u16> = Vec::new();

    for face in &faces {
        // Resolve each face vertex's position (1-indexed in OBJ, may be
        // negative to mean "relative to current count").
        let resolved_pos: Vec<[f32; 3]> = face
            .iter()
            .map(|v| resolve_index(v.pos_idx, &positions, "position"))
            .collect::<Result<Vec<_>, _>>()?;
        let resolved_nrm: Vec<[f32; 3]> = if face.iter().all(|v| v.nrm_idx.is_some()) {
            face.iter()
                .map(|v| resolve_index(v.nrm_idx.unwrap(), &normals, "normal"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            // Synthesize a face normal from the first 3 positions.
            let n = face_normal(&resolved_pos);
            vec![n; face.len()]
        };

        // Add the face's vertices to the output, triangulate as a fan.
        let base = out_positions.len() as u16;
        for i in 0..face.len() {
            out_positions.push(resolved_pos[i]);
            out_normals.push(resolved_nrm[i]);
        }
        for i in 1..(face.len() - 1) {
            out_indices.push(base);
            out_indices.push(base + i as u16);
            out_indices.push(base + (i + 1) as u16);
        }
    }

    Ok(ObjModel {
        positions: out_positions,
        normals: out_normals,
        indices: out_indices,
    })
}

#[derive(Debug, Clone, Copy)]
struct ObjFaceIndex {
    pos_idx: i32,         // 1-indexed (OBJ convention), may be negative
    nrm_idx: Option<i32>, // 1-indexed, may be None
}

fn parse_face_vertex(tok: &str) -> Result<ObjFaceIndex, String> {
    let parts: Vec<&str> = tok.split('/').collect();
    if parts.is_empty() {
        return Err(format!("empty face vertex: {}", tok));
    }
    let pos_idx: i32 = parts[0].parse()
        .map_err(|e| format!("position index {}: {}", tok, e))?;
    let nrm_idx = if parts.len() >= 3 && !parts[2].is_empty() {
        parts[2].parse::<i32>().ok()
    } else {
        None
    };
    Ok(ObjFaceIndex { pos_idx, nrm_idx })
}

fn resolve_index<T: Copy>(
    idx: i32,
    source: &[T],
    kind: &str,
) -> Result<T, String> {
    if idx == 0 {
        return Err(format!("zero {} index (OBJ is 1-indexed)", kind));
    }
    let resolved = if idx > 0 {
        (idx as usize) - 1
    } else {
        // Negative index — relative to end.
        let abs = (-idx) as usize;
        if abs > source.len() {
            return Err(format!("negative {} index {} out of range", kind, idx));
        }
        source.len() - abs
    };
    source.get(resolved).copied().ok_or_else(|| {
        format!("{} index {} out of range (have {})", kind, idx, source.len())
    })
}

/// Compute a face normal from the first 3 positions using the
/// right-hand rule: cross(p1-p0, p2-p0). Returns [0,0,1] if the
/// positions are degenerate (collinear or coincident).
fn face_normal(positions: &[[f32; 3]]) -> [f32; 3] {
    if positions.len() < 3 {
        return [0.0, 0.0, 1.0];
    }
    let a = positions[0];
    let b = positions[1];
    let c = positions[2];
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len < 1e-9 {
        return [0.0, 0.0, 1.0];
    }
    [nx / len, ny / len, nz / len]
}

/// Embedded test asset: an octahedron (8 triangular faces, 6 vertices).
/// A clean convex shape whose OBJ is small enough to embed inline.
/// Useful for testing the parser without a disk asset pipeline.
pub const OCTAHEDRON_OBJ: &str = r#"# Octahedron — 6 vertices, 8 faces
v  1.0  0.0  0.0
v -1.0  0.0  0.0
v  0.0  1.0  0.0
v  0.0 -1.0  0.0
v  0.0  0.0  1.0
v  0.0  0.0 -1.0
f 1 3 5
f 3 2 5
f 2 4 5
f 4 1 5
f 3 1 6
f 2 3 6
f 4 2 6
f 1 4 6
"#;

pub fn library_version() -> &'static str { "0.1.0" }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_octahedron() {
        let model = parse_obj(OCTAHEDRON_OBJ).expect("parse");
        assert_eq!(model.positions.len(), 24, "8 faces * 3 verts = 24");
        assert_eq!(model.triangle_count(), 8);
        assert_eq!(model.indices.len(), 24);
    }

    #[test]
    fn parse_skips_unsupported_directives() {
        let src = r#"
# A comment
o MyObject
v 0 0 0
v 1 0 0
v 0 1 0
vn 0 0 1
vt 0 0
usemtl Default
s off
f 1 2 3
"#;
        let model = parse_obj(src).expect("parse");
        assert_eq!(model.positions.len(), 3);
        assert_eq!(model.triangle_count(), 1);
    }

    #[test]
    fn parse_triangulates_quad_face() {
        let src = r#"
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
f 1 2 3 4
"#;
        let model = parse_obj(src).expect("parse");
        assert_eq!(model.triangle_count(), 2, "quad → 2 tris via fan");
        assert_eq!(model.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn parse_synthesizes_face_normal_when_missing() {
        // CCW winding from +Z: v0(0,0,0), v1(1,0,0), v2(0,1,0). Cross
        // product (v1-v0) x (v2-v0) = (1,0,0) x (0,1,0) = (0,0,1).
        let src = r#"
v 0 0 0
v 1 0 0
v 0 1 0
f 1 2 3
"#;
        let model = parse_obj(src).expect("parse");
        let n = model.normals[0];
        assert!((n[0] - 0.0).abs() < 1e-5, "got x={} expected 0", n[0]);
        assert!((n[1] - 0.0).abs() < 1e-5, "got y={} expected 0", n[1]);
        assert!((n[2] - 1.0).abs() < 1e-5, "got z={} expected 1", n[2]);
    }

    #[test]
    fn parse_uses_provided_normals() {
        let src = r#"
v 0 0 0
v 0 1 0
v 1 0 0
vn 0.6 0.0 0.8
f 1//1 2//1 3//1
"#;
        let model = parse_obj(src).expect("parse");
        // Should use the provided normal (0.6, 0, 0.8) for all 3 verts.
        for n in &model.normals {
            assert!((n[0] - 0.6).abs() < 1e-5);
            assert!((n[1] - 0.0).abs() < 1e-5);
            assert!((n[2] - 0.8).abs() < 1e-5);
        }
    }

    #[test]
    fn parse_negative_index_resolves_relative_to_end() {
        let src = r#"
v 0 0 0
v 1 0 0
v 0 1 0
f -3 -2 -1
"#;
        let model = parse_obj(src).expect("parse");
        // -3 → positions[0], -2 → positions[1], -1 → positions[2].
        assert_eq!(model.positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(model.positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(model.positions[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn parse_reports_line_number_on_error() {
        let src = "v 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let err = parse_obj(src).expect_err("should fail on line 1");
        assert!(err.contains("line 1"), "error: {}", err);
    }
}
