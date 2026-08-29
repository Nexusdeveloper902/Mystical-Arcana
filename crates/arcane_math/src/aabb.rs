//! Axis-aligned bounding boxes (AABB). Used for frustum culling, physics
//! broadphase, building placement, and chunk spatial indexing.

use crate::vec::Vec3;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// A 3D axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Aabb {
    /// Minimum corner (inclusive).
    pub min: Vec3,
    /// Maximum corner (inclusive).
    pub max: Vec3,
}

/// Convenience alias.
pub type Aabb3d = Aabb;

/// A 2D axis-aligned bounding box (useful for chunk grid queries).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Pod, Zeroable)]
#[repr(C)]
pub struct Aabb2d {
    /// Minimum corner (inclusive).
    pub min_x: f32,
    /// Minimum corner (inclusive).
    pub min_y: f32,
    /// Maximum corner (inclusive).
    pub max_x: f32,
    /// Maximum corner (inclusive).
    pub max_y: f32,
}

impl Aabb {
    /// Constructs from explicit min/max.
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Constructs from center + half-extents.
    pub fn from_center_half(center: Vec3, half: Vec3) -> Self {
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    /// Empty AABB (min > max). Useful as a starting point for `union_with`.
    pub const EMPTY: Self = Self::new(
        Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
    );

    /// True if the AABB has no extent (min > max on any axis).
    pub fn is_empty(self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    /// Center point.
    pub fn center(self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    /// Half-extent (center to max).
    pub fn half_extent(self) -> Vec3 {
        Vec3::new(
            (self.max.x - self.min.x) * 0.5,
            (self.max.y - self.min.y) * 0.5,
            (self.max.z - self.min.z) * 0.5,
        )
    }

    /// Total extent (max - min).
    pub fn extent(self) -> Vec3 {
        Vec3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// Expands the AABB to contain `p`.
    pub fn union_point(mut self, p: Vec3) -> Self {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
        self
    }

    /// Expands the AABB to contain `other`.
    pub fn union_with(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// True if `p` is inside this AABB (inclusive bounds).
    pub fn contains_point(self, p: Vec3) -> bool {
        p.x >= self.min.x && p.x <= self.max.x
            && p.y >= self.min.y && p.y <= self.max.y
            && p.z >= self.min.z && p.z <= self.max.z
    }

    /// True if `other` is entirely inside this AABB.
    pub fn contains_aabb(self, other: Self) -> bool {
        self.min.x <= other.min.x && self.max.x >= other.max.x
            && self.min.y <= other.min.y && self.max.y >= other.max.y
            && self.min.z <= other.min.z && self.max.z >= other.max.z
    }

    /// True if `other` intersects this AABB (inclusive bounds).
    pub fn intersects(self, other: Self) -> bool {
        !(self.max.x < other.min.x || self.min.x > other.max.x
            || self.max.y < other.min.y || self.min.y > other.max.y
            || self.max.z < other.min.z || self.min.z > other.max.z)
    }

    /// Expands all sides by `padding`.
    pub fn expand(self, padding: f32) -> Self {
        Self {
            min: Vec3::new(self.min.x - padding, self.min.y - padding, self.min.z - padding),
            max: Vec3::new(self.max.x + padding, self.max.y + padding, self.max.z + padding),
        }
    }
}

impl Aabb2d {
    /// Constructs from explicit min/max.
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }

    /// Center coordinates.
    pub fn center(self) -> (f32, f32) {
        ((self.min_x + self.max_x) * 0.5, (self.min_y + self.max_y) * 0.5)
    }

    /// True if `other` intersects this 2D AABB.
    pub fn intersects(self, other: Self) -> bool {
        !(self.max_x < other.min_x || self.min_x > other.max_x
            || self.max_y < other.min_y || self.min_y > other.max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb_unit() -> Aabb {
        Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0))
    }

    #[test]
    fn aabb_center_half_extent() {
        let a = aabb_unit();
        let c = a.center();
        assert!((c.x - 0.0).abs() < 1e-6);
        assert!((c.y - 0.0).abs() < 1e-6);
        assert!((c.z - 0.0).abs() < 1e-6);
        let h = a.half_extent();
        assert!((h.x - 1.0).abs() < 1e-6);
        assert!((h.y - 1.0).abs() < 1e-6);
        assert!((h.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn aabb_contains_point() {
        let a = aabb_unit();
        assert!(a.contains_point(Vec3::ZERO));
        assert!(a.contains_point(Vec3::new(0.5, 0.5, 0.5)));
        assert!(a.contains_point(Vec3::new(1.0, 1.0, 1.0))); // inclusive
        assert!(!a.contains_point(Vec3::new(1.5, 0.0, 0.0)));
        assert!(!a.contains_point(Vec3::new(0.0, -1.5, 0.0)));
    }

    #[test]
    fn aabb_intersects() {
        let a = aabb_unit();
        let b = Aabb::new(Vec3::new(0.5, 0.5, 0.5), Vec3::new(2.0, 2.0, 2.0));
        assert!(a.intersects(b));
        let c = Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0));
        assert!(!a.intersects(c)); // touching corner only — inclusive check below
        // Edge case: touching at a single point is technically inside, since bounds are inclusive.
        let d = Aabb::new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 2.0, 2.0));
        assert!(a.intersects(d));
    }

    #[test]
    fn aabb_contains_aabb() {
        let outer = Aabb::new(Vec3::new(-10.0, -10.0, -10.0), Vec3::new(10.0, 10.0, 10.0));
        let inner = aabb_unit();
        assert!(outer.contains_aabb(inner));
        assert!(!inner.contains_aabb(outer));
    }

    #[test]
    fn aabb_union_with_point() {
        let a = aabb_unit();
        let b = a.union_point(Vec3::new(5.0, -5.0, 0.0));
        assert_eq!(b.max.x, 5.0);
        assert_eq!(b.min.y, -5.0);
    }

    #[test]
    fn aabb_union_with_aabb() {
        let a = aabb_unit();
        let b = Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(5.0, 5.0, 5.0));
        let u = a.union_with(b);
        assert_eq!(u.min, Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(u.max, Vec3::new(5.0, 5.0, 5.0));
    }

    #[test]
    fn aabb_empty_is_inverted() {
        let e = Aabb::EMPTY;
        assert!(e.is_empty());
    }

    #[test]
    fn aabb_expand() {
        let a = aabb_unit();
        let e = a.expand(2.0);
        assert_eq!(e.min, Vec3::new(-3.0, -3.0, -3.0));
        assert_eq!(e.max, Vec3::new(3.0, 3.0, 3.0));
    }

    #[test]
    fn aabb2d_intersects() {
        let a = Aabb2d::new(0.0, 0.0, 1.0, 1.0);
        let b = Aabb2d::new(0.5, 0.5, 1.5, 1.5);
        assert!(a.intersects(b));
        let c = Aabb2d::new(2.0, 2.0, 3.0, 3.0);
        assert!(!a.intersects(c));
    }
}
