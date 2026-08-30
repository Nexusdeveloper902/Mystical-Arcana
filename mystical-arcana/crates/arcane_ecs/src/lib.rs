//! arcane_ecs — entity component system.
//!
//! A very small archetype-style registry. We avoid pulling in a heavy ECS
//! dependency so we can render frames deterministically without external
//! ordering surprises.

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
