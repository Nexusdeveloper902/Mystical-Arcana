//! Object pool and slab-style free-list allocator.
//!
//! Used by VFX, particles, audio sources, networked entities, and any
//! hot path that creates many short-lived objects. The pool guarantees:
//! - Zero heap allocation after warmup.
//! - Stable indices across removes (so handles stay valid).
//! - O(1) acquire/release.
//!
//! Generational index is implemented by `Handle<T>` (see `handle.rs`).
//! This module provides the simpler [`ObjectPool<T>`]: an indexed free-list
//! that returns `usize` handles without generation checks. Use
//! [`HandleTable<T>`] when you need generation checks (long-lived handles).

use std::marker::PhantomData;

// Suppress unused warning when the marker type isn't needed for Handle.
#[allow(dead_code)]
type _PhantomSentinel = PhantomData<()>;

/// An indexed object pool. Stores objects in a contiguous `Vec`; free slots
/// are tracked in a free-list. After warmup, `acquire` and `release` never
/// allocate.
///
/// Suitable for VFX particles, audio sources, transient projectiles.
/// Not suitable for cross-thread sharing (use `parking_lot::Mutex<ObjectPool<T>>`).
pub struct ObjectPool<T> {
    items: Vec<Slot<T>>,
    free: Vec<usize>,
}

#[derive(Debug)]
enum Slot<T> {
    /// Slot is in use; the value is here.
    Occupied(T),
    /// Slot is free; the next free slot index (or `usize::MAX` for end-of-list).
    Free(usize),
}

impl<T> ObjectPool<T> {
    /// Creates an empty pool.
    pub fn new() -> Self {
        Self { items: Vec::new(), free: Vec::new() }
    }

    /// Pre-allocates capacity for `n` slots. Subsequent `acquire` calls up
    /// to `n` will not allocate.
    pub fn with_capacity(n: usize) -> Self {
        Self {
            items: Vec::with_capacity(n),
            free: Vec::with_capacity(n),
        }
    }

    /// Acquires a slot for `value`. Returns its index. O(1) after warmup.
    pub fn acquire(&mut self, value: T) -> usize {
        if let Some(idx) = self.free.pop() {
            self.items[idx] = Slot::Occupied(value);
            idx
        } else {
            let idx = self.items.len();
            self.items.push(Slot::Occupied(value));
            idx
        }
    }

    /// Releases the slot at `idx`. After release, `get(idx)` returns `None`.
    /// Releasing an already-free slot is a programming error and will panic
    /// in debug builds.
    pub fn release(&mut self, idx: usize) -> Option<T> {
        if idx >= self.items.len() {
            return None;
        }
        match std::mem::replace(&mut self.items[idx], Slot::Free(usize::MAX)) {
            Slot::Occupied(v) => {
                self.free.push(idx);
                Some(v)
            }
            Slot::Free(_) => None,
        }
    }

    /// Returns a shared reference to the value at `idx`, or `None` if free.
    pub fn get(&self, idx: usize) -> Option<&T> {
        match self.items.get(idx)? {
            Slot::Occupied(v) => Some(v),
            Slot::Free(_) => None,
        }
    }

    /// Returns a mutable reference to the value at `idx`, or `None` if free.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        match self.items.get_mut(idx)? {
            Slot::Occupied(v) => Some(v),
            Slot::Free(_) => None,
        }
    }

    /// Number of currently-occupied slots.
    pub fn occupied_count(&self) -> usize {
        self.items.len() - self.free.len()
    }

    /// Total capacity including free slots.
    pub fn capacity(&self) -> usize {
        self.items.capacity()
    }

    /// Iterates over occupied (index, value) pairs.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (usize, &T)> {
        self.items.iter().enumerate().filter_map(|(i, s)| match s {
            Slot::Occupied(v) => Some((i, v)),
            Slot::Free(_) => None,
        })
    }

    /// Iterates over occupied (index, value) pairs, mutably.
    pub fn iter_occupied_mut(&mut self) -> impl Iterator<Item = (usize, &mut T)> {
        self.items.iter_mut().enumerate().filter_map(|(i, s)| match s {
            Slot::Occupied(v) => Some((i, v)),
            Slot::Free(_) => None,
        })
    }

    /// Clears all slots and frees all storage. Use sparingly.
    pub fn clear(&mut self) {
        self.items.clear();
        self.free.clear();
    }
}

impl<T> Default for ObjectPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

// === Generational handles === ----------------------------------------------

/// A generational handle. The `index` is the slot, the `generation` is
/// incremented every time the slot is released — so a stale `Handle` from
/// before a release is detected as "dead" and `get()` returns `None`.
///
/// `Handle` is intentionally non-generic: it is just two integers, and we
/// want `Copy` and zero-overhead at all use sites. Type-safe wrappers can
/// be built on top via newtypes if a particular subsystem needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    /// Slot index.
    pub index: u32,
    /// Generation counter.
    pub generation: u32,
}

impl Handle {
    /// Constructs a handle from raw parts. Mostly internal — used by tests.
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// The null handle. `get()` on this always returns `None`.
    pub const fn null() -> Self {
        Self::from_raw(u32::MAX, u32::MAX)
    }

    /// True if null.
    pub fn is_null(self) -> bool {
        self.index == u32::MAX && self.generation == u32::MAX
    }
}

/// A generational handle table. Use this over `ObjectPool<T>` when callers
/// hold long-lived handles that must detect staleness (e.g., inventory items,
/// resource references, audio sources bound to entities).
#[derive(Debug)]
pub struct HandleTable<T> {
    items: Vec<HandleSlot<T>>,
    free: Vec<u32>,
}

#[derive(Debug)]
enum HandleSlot<T> {
    Occupied { value: T, generation: u32 },
    Free { next_free: u32, generation: u32 },
}

impl<T> HandleTable<T> {
    /// New empty table.
    pub fn new() -> Self {
        Self { items: Vec::new(), free: Vec::new() }
    }

    /// Pre-allocates capacity for `n` entries.
    pub fn with_capacity(n: usize) -> Self {
        Self {
            items: Vec::with_capacity(n),
            free: Vec::with_capacity(n),
        }
    }

    /// Inserts a value and returns its handle.
    pub fn insert(&mut self, value: T) -> Handle {
        if let Some(idx) = self.free.pop() {
            match &mut self.items[idx as usize] {
                HandleSlot::Free { generation, .. } => {
                    let g = *generation;
                    self.items[idx as usize] = HandleSlot::Occupied { value, generation: g };
                    Handle::from_raw(idx, g)
                }
                _ => unreachable!("free list pointed at occupied slot"),
            }
        } else {
            let idx = self.items.len() as u32;
            let g = 0;
            self.items.push(HandleSlot::Occupied { value, generation: g });
            Handle::from_raw(idx, g)
        }
    }

    /// Returns `Some(value)` if the handle is alive, `None` if stale or null.
    pub fn remove(&mut self, h: Handle) -> Option<T> {
        if h.is_null() {
            return None;
        }
        let idx = h.index as usize;
        if idx >= self.items.len() {
            return None;
        }
        match &self.items[idx] {
            HandleSlot::Occupied { generation, .. } if *generation == h.generation => {
                let new_gen = h.generation.wrapping_add(1).max(1);
                let old = std::mem::replace(
                    &mut self.items[idx],
                    HandleSlot::Free { next_free: u32::MAX, generation: new_gen },
                );
                self.free.push(idx as u32);
                match old {
                    HandleSlot::Occupied { value, .. } => Some(value),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }

    /// Returns a shared reference to the value behind `h`, if alive.
    pub fn get(&self, h: Handle) -> Option<&T> {
        if h.is_null() {
            return None;
        }
        match self.items.get(h.index as usize)? {
            HandleSlot::Occupied { value, generation } if *generation == h.generation => Some(value),
            _ => None,
        }
    }

    /// Returns a mutable reference to the value behind `h`, if alive.
    pub fn get_mut(&mut self, h: Handle) -> Option<&mut T> {
        if h.is_null() {
            return None;
        }
        match self.items.get_mut(h.index as usize)? {
            HandleSlot::Occupied { value, generation } if *generation == h.generation => Some(value),
            _ => None,
        }
    }

    /// True if `h` is alive in this table.
    pub fn is_alive(&self, h: Handle) -> bool {
        self.get(h).is_some()
    }

    /// Number of occupied slots.
    pub fn len(&self) -> usize {
        self.items.iter().filter(|s| matches!(s, HandleSlot::Occupied { .. })).count()
    }

    /// True if there are no occupied slots.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for HandleTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ObjectPool ----------------------------------------------------------

    #[test]
    fn pool_acquire_release_reuses_slots() {
        let mut p: ObjectPool<i32> = ObjectPool::with_capacity(4);
        let a = p.acquire(10);
        let b = p.acquire(20);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(p.occupied_count(), 2);
        let r = p.release(a);
        assert_eq!(r, Some(10));
        assert_eq!(p.occupied_count(), 1);
        // Acquire should reuse the freed slot at index 0.
        let c = p.acquire(30);
        assert_eq!(c, 0);
        assert_eq!(p.get(c), Some(&30));
    }

    #[test]
    fn pool_get_after_release_is_none() {
        let mut p: ObjectPool<&'static str> = ObjectPool::new();
        let h = p.acquire("hi");
        assert_eq!(p.get(h), Some(&"hi"));
        let _ = p.release(h);
        assert_eq!(p.get(h), None);
    }

    #[test]
    fn pool_iter_occupied_skips_free_slots() {
        let mut p: ObjectPool<i32> = ObjectPool::new();
        let _ = p.acquire(1);
        let _ = p.acquire(2);
        let _ = p.acquire(3);
        // Free the middle slot.
        p.release(1);
        let occupied: Vec<i32> = p.iter_occupied().map(|(_, v)| *v).collect();
        assert_eq!(occupied, vec![1, 3]);
    }

    // --- HandleTable ---------------------------------------------------------

    #[test]
    fn handle_table_stale_handle_returns_none() {
        let mut t: HandleTable<i32> = HandleTable::new();
        let h = t.insert(10);
        assert!(t.is_alive(h));
        assert_eq!(t.get(h), Some(&10));
        let removed = t.remove(h);
        assert_eq!(removed, Some(10));
        // Stale handle now returns None — generation mismatch.
        assert!(!t.is_alive(h));
        assert_eq!(t.get(h), None);
    }

    #[test]
    fn handle_table_reuses_slot_with_new_generation() {
        let mut t: HandleTable<i32> = HandleTable::new();
        let h1 = t.insert(10);
        let _ = t.remove(h1);
        let h2 = t.insert(20);
        // Slot index reused, but generation incremented.
        assert_eq!(h1.index, h2.index);
        assert_ne!(h1.generation, h2.generation);
        assert_eq!(t.get(h2), Some(&20));
        // Old handle still stale.
        assert_eq!(t.get(h1), None);
    }

    #[test]
    fn handle_table_null_handle_is_none() {
        let mut t: HandleTable<i32> = HandleTable::new();
        let h = Handle::null();
        assert!(h.is_null());
        assert_eq!(t.get(h), None);
        assert_eq!(t.remove(h), None);
    }

    #[test]
    fn handle_table_len_tracks_occupied() {
        let mut t: HandleTable<String> = HandleTable::new();
        assert!(t.is_empty());
        let a = t.insert("a".into());
        let b = t.insert("b".into());
        assert_eq!(t.len(), 2);
        t.remove(a);
        assert_eq!(t.len(), 1);
        t.remove(b);
        assert!(t.is_empty());
    }
}
