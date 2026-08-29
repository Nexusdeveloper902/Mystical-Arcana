//! Frustum culling primitives.
//!
//! Used by the Arcane Renderer's visibility pass. A frustum is built from a
//! view-projection matrix; AABBs and points are tested against its 6 planes.
//!
//! Implementation: Gribb-Hartmann plane extraction (standard graphics technique).

use crate::aabb::Aabb;
use crate::vec::{Vec3, Vec4};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// A frustum plane (normal + distance from origin).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct FrustumPlane {
    /// Plane normal (points outward).
    pub normal: Vec3,
    /// Distance from origin to the plane.
    pub d: f32,
}

impl FrustumPlane {
    /// Signed distance from `point` to this plane. Positive = inside (in front).
    pub fn distance_to(self, point: Vec3) -> f32 {
        self.normal.x * point.x + self.normal.y * point.y + self.normal.z * point.z + self.d
    }
}

/// A 6-plane frustum.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Frustum {
    /// Left plane.
    pub left: FrustumPlane,
    /// Right plane.
    pub right: FrustumPlane,
    /// Bottom plane.
    pub bottom: FrustumPlane,
    /// Top plane.
    pub top: FrustumPlane,
    /// Near plane.
    pub near: FrustumPlane,
    /// Far plane.
    pub far: FrustumPlane,
}

impl Frustum {
    /// Extracts the 6 frustum planes from a view-projection matrix.
    /// `vp` is row-major or column-major — nalgebra uses column-major, which
    /// is what we expect here.
    pub fn from_view_proj(vp: &nalgebra::Matrix4<f32>) -> Self {
        // nalgebra is column-major. Row r column c access is `vp[(c, r)]`.
        // Gribb-Hartmann needs the rows of (M^T) — which equal the columns of M
        // in column-major storage. We access rows directly via `vp.row(r)`.
        let m = vp;

        // Row accessors.
        let r0 = Vec4::new(m[(0, 0)], m[(0, 1)], m[(0, 2)], m[(0, 3)]);
        let r1 = Vec4::new(m[(1, 0)], m[(1, 1)], m[(1, 2)], m[(1, 3)]);
        let r2 = Vec4::new(m[(2, 0)], m[(2, 1)], m[(2, 2)], m[(2, 3)]);
        let r3 = Vec4::new(m[(3, 0)], m[(3, 1)], m[(3, 2)], m[(3, 3)]);

        let mk_plane = |p: Vec4| {
            let len = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
            if len < 1e-9 {
                FrustumPlane { normal: Vec3::ZERO, d: 0.0 }
            } else {
                FrustumPlane {
                    normal: Vec3::new(p.x / len, p.y / len, p.z / len),
                    d: p.w / len,
                }
            }
        };

        // Planes (in nalgebra column-major, the rows of M*M^T are columns of M).
        // Gribb-Hartmann standard formulas:
        //   left   = r3 + r0
        //   right  = r3 - r0
        //   bottom = r3 + r1
        //   top    = r3 - r1
        //   near   = r3 + r2
        //   far    = r3 - r2
        let left = mk_plane(add(r3, r0));
        let right = mk_plane(sub(r3, r0));
        let bottom = mk_plane(add(r3, r1));
        let top = mk_plane(sub(r3, r1));
        let near = mk_plane(add(r3, r2));
        let far = mk_plane(sub(r3, r2));

        Frustum { left, right, bottom, top, near, far }
    }

    /// Iterates the 6 planes.
    fn planes(&self) -> [FrustumPlane; 6] {
        [self.left, self.right, self.bottom, self.top, self.near, self.far]
    }

    /// True if `point` is inside the frustum (i.e. on the inside of every plane).
    pub fn contains_point(self, point: Vec3) -> bool {
        for p in self.planes() {
            if p.distance_to(point) < 0.0 {
                return false;
            }
        }
        true
    }

    /// Conservative AABB-vs-frustum intersection test. Returns true if the
    /// AABB might be visible (i.e. intersects the frustum or is fully inside it).
    /// Uses the "positive vertex" trick: for each plane, find the AABB corner
    /// furthest along the plane normal; if that corner is outside the plane,
    /// the AABB is outside the frustum.
    pub fn intersects_aabb(self, aabb: Aabb) -> bool {
        // Cache AABB corners' min/max per axis.
        let min = aabb.min;
        let max = aabb.max;

        for plane in self.planes() {
            // "Positive vertex" = the corner of the AABB furthest in the
            // direction of the plane normal.
            let px = if plane.normal.x > 0.0 { max.x } else { min.x };
            let py = if plane.normal.y > 0.0 { max.y } else { min.y };
            let pz = if plane.normal.z > 0.0 { max.z } else { min.z };
            let positive = Vec3::new(px, py, pz);
            if plane.distance_to(positive) < 0.0 {
                return false;
            }
        }
        true
    }
}

fn add(a: Vec4, b: Vec4) -> Vec4 {
    Vec4::new(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w)
}
fn sub(a: Vec4, b: Vec4) -> Vec4 {
    Vec4::new(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aabb::Aabb;
    use crate::vec::Vec3;

    fn ortho_view_proj(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> nalgebra::Matrix4<f32> {
        // Builds an orthographic projection. The view is identity; vp = proj.
        let mut m = nalgebra::Matrix4::<f32>::identity();
        m[(0, 0)] = 2.0 / (r - l);
        m[(1, 1)] = 2.0 / (t - b);
        m[(2, 2)] = -2.0 / (f - n);
        m[(0, 3)] = -(r + l) / (r - l);
        m[(1, 3)] = -(t + b) / (t - b);
        m[(2, 3)] = -(f + n) / (f - n);
        m
    }

    #[test]
    fn frustum_contains_center_of_orthographic() {
        let vp = ortho_view_proj(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
        let frustum = Frustum::from_view_proj(&vp);
        assert!(frustum.contains_point(Vec3::ZERO), "origin should be inside the frustum");
    }

    #[test]
    fn frustum_excludes_far_outside_point() {
        let vp = ortho_view_proj(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
        let frustum = Frustum::from_view_proj(&vp);
        assert!(!frustum.contains_point(Vec3::new(5.0, 0.0, 0.0)), "distant point should be outside");
    }

    #[test]
    fn frustum_intersects_aabb_inside() {
        let vp = ortho_view_proj(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
        let frustum = Frustum::from_view_proj(&vp);
        let a = Aabb::new(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        assert!(frustum.intersects_aabb(a), "AABB inside frustum should be visible");
    }

    #[test]
    fn frustum_culls_distant_aabb() {
        let vp = ortho_view_proj(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
        let frustum = Frustum::from_view_proj(&vp);
        let a = Aabb::new(Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0));
        assert!(!frustum.intersects_aabb(a), "distant AABB should be culled");
    }

    #[test]
    fn frustum_partial_overlap_returns_visible() {
        let vp = ortho_view_proj(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
        let frustum = Frustum::from_view_proj(&vp);
        // AABB straddles the frustum edge — partially visible.
        let a = Aabb::new(Vec3::new(0.5, -0.5, -0.5), Vec3::new(5.0, 0.5, 0.5));
        assert!(frustum.intersects_aabb(a), "overlapping AABB should be visible");
    }
}
