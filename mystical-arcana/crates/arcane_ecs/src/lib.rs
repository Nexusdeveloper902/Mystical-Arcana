//! arcane_ecs — entity component system.
//!
//! A very small archetype-style registry. We avoid pulling in a heavy ECS
//! dependency so we can render frames deterministically without external
//! ordering surprises.
//!
//! Components are stored as `Box<dyn Any + Send + Sync>` per entity. To
//! read a component back, call `get::<T>(e)` (which downcasts) — works for
//! any `T: Any + Send + Sync`.

use std::any::Any;
use std::any::TypeId;
use std::collections::HashMap;
use parking_lot::RwLock;

pub type Entity = u64;

#[derive(Default)]
pub struct World {
    next_id: Entity,
    entities: HashMap<Entity, Vec<Box<dyn Any + Send + Sync>>>,
}

impl World {
    pub fn new() -> Self { Self::default() }

    pub fn spawn(&mut self) -> Entity {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(id, Vec::new());
        id
    }

    pub fn despawn(&mut self, e: Entity) {
        self.entities.remove(&e);
    }

    pub fn attach<T: Any + Send + Sync>(&mut self, e: Entity, c: T) {
        if let Some(v) = self.entities.get_mut(&e) {
            v.push(Box::new(c));
        }
    }

    pub fn count(&self) -> usize {
        self.entities.len()
    }

    /// Read-only access to the first component of type `T` on entity `e`.
    /// Returns None if the entity has no such component.
    pub fn get<T: Any + Send + Sync>(&self, e: Entity) -> Option<&T> {
        self.entities.get(&e).and_then(|v| {
            v.iter().find_map(|c| c.downcast_ref::<T>())
        })
    }

    /// Mutable access to the first component of type `T` on entity `e`.
    pub fn get_mut<T: Any + Send + Sync>(&mut self, e: Entity) -> Option<&mut T> {
        self.entities.get_mut(&e).and_then(|v| {
            v.iter_mut().find_map(|c| c.downcast_mut::<T>())
        })
    }

    /// Collect all entities that currently have at least one component
    /// of type `T`. Useful as the query primitive for a system that
    /// iterates entities-with-Transform.
    pub fn entities_with<T: Any + Send + Sync>(&self) -> Vec<Entity> {
        self.entities.iter()
            .filter_map(|(e, v)| {
                if v.iter().any(|c| c.is::<T>()) { Some(*e) } else { None }
            })
            .collect()
    }

    /// Total number of components of type `T` across all entities.
    /// Mostly useful for sanity-checking test setups.
    pub fn count_components<T: Any + Send + Sync>(&self) -> usize {
        self.entities.values()
            .map(|v| v.iter().filter(|c| c.is::<T>()).count())
            .sum()
    }
}

pub struct WorldRef<'a> {
    inner: parking_lot::RwLockReadGuard<'a, World>,
}

pub struct WorldMut<'a> {
    inner: parking_lot::RwLockWriteGuard<'a, World>,
}

pub struct ConcurrentWorld {
    inner: RwLock<World>,
}

impl ConcurrentWorld {
    pub fn new() -> Self {
        Self { inner: RwLock::new(World::new()) }
    }
    pub fn read(&self) -> WorldRef<'_> { WorldRef { inner: self.inner.read() } }
    pub fn write(&self) -> WorldMut<'_> { WorldMut { inner: self.inner.write() } }
}

impl Default for ConcurrentWorld { fn default() -> Self { Self::new() } }

// Suppress unused-import warning; these types are exposed for future extension.
#[allow(dead_code)]
fn _silence_unused() {
    let _ = TypeId::of::<()>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Health(i32);
    #[derive(Debug, PartialEq)]
    struct Name(String);

    #[test]
    fn attach_and_get() {
        let mut w = World::new();
        let e = w.spawn();
        w.attach(e, Health(100));
        w.attach(e, Name("hero".to_string()));
        assert_eq!(w.get::<Health>(e), Some(&Health(100)));
        assert_eq!(w.get::<Name>(e).unwrap().0, "hero");
    }

    #[test]
    fn get_mut_updates_in_place() {
        let mut w = World::new();
        let e = w.spawn();
        w.attach(e, Health(50));
        if let Some(h) = w.get_mut::<Health>(e) {
            h.0 -= 10;
        }
        assert_eq!(w.get::<Health>(e), Some(&Health(40)));
    }

    #[test]
    fn entities_with_filters_by_component_type() {
        let mut w = World::new();
        let e1 = w.spawn();
        let e2 = w.spawn();
        let e3 = w.spawn();
        w.attach(e1, Health(1));
        w.attach(e2, Health(2));
        w.attach(e3, Name("no health".to_string()));
        let with_health = w.entities_with::<Health>();
        assert_eq!(with_health.len(), 2);
        assert!(with_health.contains(&e1));
        assert!(with_health.contains(&e2));
        assert!(!with_health.contains(&e3));
    }
}
