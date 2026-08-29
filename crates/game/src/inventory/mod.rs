//! Inventory, items, stacks, pickups — data-driven item definitions.
//!
//! Per the design doc:
//!   "Inventory should remain subordinate to the game's magical identity.
//!    We're not trying to create a spreadsheet simulator."
//!
//! Items the player carries are the *ingredients* to interact with the world.
//! Stacks of identical items combine up to a max stack size. Tablets and
//! magical materials have unique rules (tablets don't stack — each is
//! tied to a specific rune).

use arcane_core::{Handle, HandleTable, Id64};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum default stack size for stackable items.
pub const DEFAULT_MAX_STACK: u32 = 99;

/// Item category — affects how the item interacts with other systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ItemKind {
    /// A raw material gathered from the world (wood, stone, mana dust, etc.).
    Resource = 0,
    /// A refined material produced by crafting/sanctuaries.
    Material = 1,
    /// A rune tablet — non-stackable, tied to a specific rune.
    RuneTablet = 2,
    /// A consumable potion or magical dose.
    Consumable = 3,
    /// A quest/lore item — not consumed by normal means.
    QuestItem = 4,
    /// A tool the player once used — but cannot anymore (lore / story).
    DecayedTool = 5,
}

/// A complete item definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    /// Stable string id — e.g. "wood", "mana_dust".
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Item kind.
    pub kind: ItemKind,
    /// Max stack size. 1 for non-stackable items (e.g. rune tablets).
    pub max_stack: u32,
    /// Optional: the rune this tablet is tied to (only for RuneTablet).
    pub tablet_rune: Option<Id64>,
    /// Optional: market value for trade (not all items have one).
    pub value: Option<u32>,
    /// Optional: icon path within the asset system.
    pub icon_path: Option<String>,
}

impl ItemDef {
    /// Computes the stable `Id64` for this item.
    pub fn stable_id(&self) -> Id64 {
        Id64::from_str(&self.id)
    }

    /// True if this item can stack.
    pub fn is_stackable(&self) -> bool {
        self.max_stack > 1
    }
}

/// Item definitions registry.
#[derive(Debug, Default)]
pub struct ItemRegistry {
    defs: HashMap<Id64, ItemDef>,
    by_string: HashMap<String, Id64>,
}

impl ItemRegistry {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an item.
    pub fn register(&mut self, def: ItemDef) {
        let id = def.stable_id();
        self.by_string.insert(def.id.clone(), id);
        self.defs.insert(id, def);
    }

    /// Looks up by stable ID.
    pub fn get(&self, id: Id64) -> Option<&ItemDef> {
        self.defs.get(&id)
    }

    /// Looks up by string id.
    pub fn get_by_str(&self, s: &str) -> Option<&ItemDef> {
        self.by_string.get(s).and_then(|id| self.defs.get(id))
    }

    /// Number of registered item defs.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// A single stack of items in the inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    /// Stable id of the item def.
    pub item_id: Id64,
    /// Current stack count.
    pub count: u32,
}

impl ItemStack {
    /// Constructs a new stack with `count` items.
    pub fn new(item_id: Id64, count: u32) -> Self {
        Self { item_id, count }
    }
}

/// The player's inventory. Uses a `HandleTable` for slot management so that
/// external references (UI, drag-and-drop) can track slot handles and
/// detect when slots are emptied/refilled.
///
/// Serde note: we don't serialize the handle table directly; instead we
/// serialize the list of occupied stacks. This keeps the save format stable
/// across handle-table implementation changes.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
    /// All occupied stacks (a vec; HandleTable is reconstructed on load).
    #[serde(serialize_with = "serialize_slots", deserialize_with = "deserialize_slots")]
    pub slots: HandleTable<ItemStack>,
}

fn serialize_slots<S: serde::Serializer>(slots: &HandleTable<ItemStack>, s: S) -> Result<S::Ok, S::Error> {
    let stacks: Vec<&ItemStack> = collect_occupied_stacks(slots);
    stacks.serialize(s)
}

fn deserialize_slots<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HandleTable<ItemStack>, D::Error> {
    use serde::Deserialize;
    let stacks: Vec<ItemStack> = Vec::deserialize(d)?;
    let mut table = HandleTable::new();
    for s in stacks {
        table.insert(s);
    }
    Ok(table)
}

fn collect_occupied_stacks(slots: &HandleTable<ItemStack>) -> Vec<&ItemStack> {
    let mut out = Vec::with_capacity(slots.len());
    // Probe by index since HandleTable doesn't expose iter.
    let mut idx = 0u32;
    loop {
        let h = Handle::from_raw(idx, 0);
        if let Some(s) = slots.get(h) {
            out.push(s);
        }
        idx += 1;
        if idx as usize > slots.len() * 4 {
            break;
        }
    }
    out
}

impl Inventory {
    /// Empty inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `count` items of `item_id` to the inventory. Stacks intelligently
    /// into existing slots that match the item id, then opens new slots as
    /// needed. Returns the leftover count that could not be added (always 0
    /// unless a slot limit was hit; we don't impose an inventory cap here).
    pub fn add(&mut self, item_id: Id64, count: u32, reg: &ItemRegistry) -> u32 {
        let max_stack = reg.get(item_id).map(|d| d.max_stack).unwrap_or(DEFAULT_MAX_STACK).max(1);
        let mut remaining = count;

        // Fill existing matching slots first.
        // Collect slot handles, then iterate mutably (avoids borrow issues).
        let slot_handles: Vec<Handle> = self.collect_handles();
        for h in slot_handles {
            if remaining == 0 {
                break;
            }
            if let Some(stack) = self.slots.get_mut(h) {
                if stack.item_id != item_id {
                    continue;
                }
                let space = max_stack.saturating_sub(stack.count);
                if space == 0 {
                    continue;
                }
                let add = remaining.min(space);
                stack.count += add;
                remaining -= add;
            }
        }

        // Open new slots for any remaining items.
        while remaining > 0 {
            let add = remaining.min(max_stack);
            self.slots.insert(ItemStack::new(item_id, add));
            remaining -= add;
        }

        remaining
    }

    /// Returns the total count of items of a given id currently in the inventory.
    pub fn count_of(&self, item_id: Id64) -> u32 {
        let mut total = 0u32;
        for (_, stack) in self.iter() {
            if stack.item_id == item_id {
                total += stack.count;
            }
        }
        total
    }

    /// Removes up to `count` items of `item_id`. Returns the actual removed count.
    pub fn remove(&mut self, item_id: Id64, count: u32) -> u32 {
        let mut to_remove = count;
        let handles: Vec<Handle> = self.collect_handles();
        for h in handles {
            if to_remove == 0 {
                break;
            }
            // We need to inspect then mutate, so use a scoped mutate.
            let mut take = 0u32;
            if let Some(stack) = self.slots.get_mut(h) {
                if stack.item_id != item_id {
                    continue;
                }
                take = stack.count.min(to_remove);
                stack.count -= take;
                to_remove -= take;
            }
            // If the slot is now empty, remove it from the table.
            let should_remove = self.slots.get(h).map(|s| s.count == 0).unwrap_or(false);
            if should_remove {
                self.slots.remove(h);
            }
        }
        count - to_remove
    }

    /// True if the inventory contains at least `count` items of `item_id`.
    pub fn has(&self, item_id: Id64, count: u32) -> bool {
        self.count_of(item_id) >= count
    }

    /// Number of occupied slots.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Iterates over occupied slots as (handle, stack).
    pub fn iter(&self) -> impl Iterator<Item = (Handle, &ItemStack)> + '_ {
        // We need to iterate over the table's occupied slots. The HandleTable
        // doesn't expose this directly, so we scan.
        SlotIter {
            table: &self.slots,
            idx: 0,
        }
    }

    /// Collects all alive slot handles (helper for borrowing discipline).
    fn collect_handles(&self) -> Vec<Handle> {
        let mut out = Vec::with_capacity(self.slots.len());
        let mut idx = 0u32;
        // We iterate up to the items vec length by probing handles.
        loop {
            let h = Handle::from_raw(idx, 0);
            if h.index >= self.slots.len() as u32 && idx >= self.slots.len() as u32 {
                break;
            }
            if self.slots.is_alive(h) {
                out.push(h);
            }
            idx += 1;
            if idx > 1_000_000 {
                break; // safety
            }
        }
        out
    }
}

struct SlotIter<'a, T> {
    table: &'a HandleTable<T>,
    idx: u32,
}

impl<'a> Iterator for SlotIter<'a, ItemStack> {
    type Item = (Handle, &'a ItemStack);
    fn next(&mut self) -> Option<Self::Item> {
        while (self.idx as usize) < self.table.len() * 2 + 4 {
            let h = Handle::from_raw(self.idx, 0);
            if let Some(v) = self.table.get(h) {
                self.idx += 1;
                return Some((h, v));
            }
            self.idx += 1;
        }
        None
    }
}

/// Builds a default item registry with starter items.
pub fn default_item_registry() -> ItemRegistry {
    let mut reg = ItemRegistry::new();
    for def in default_items() {
        reg.register(def);
    }
    reg
}

/// Starter item set.
pub fn default_items() -> Vec<ItemDef> {
    use ItemKind::*;
    vec![
        ItemDef {
            id: "wood".into(),
            name: "Wood".into(),
            description: "A piece of fallen timber.".into(),
            kind: Resource,
            max_stack: 99,
            tablet_rune: None,
            value: Some(1),
            icon_path: Some("icons/items/wood.png".into()),
        },
        ItemDef {
            id: "stone".into(),
            name: "Stone".into(),
            description: "A rough stone.".into(),
            kind: Resource,
            max_stack: 99,
            tablet_rune: None,
            value: Some(1),
            icon_path: Some("icons/items/stone.png".into()),
        },
        ItemDef {
            id: "mana_dust".into(),
            name: "Mana Dust".into(),
            description: "Refined mana residue. Used in research and crafting.".into(),
            kind: Material,
            max_stack: 999,
            tablet_rune: None,
            value: Some(5),
            icon_path: Some("icons/items/mana_dust.png".into()),
        },
        ItemDef {
            id: "crystal_shard".into(),
            name: "Crystal Shard".into(),
            description: "A shard of pure crystal, resonant with mana.".into(),
            kind: Material,
            max_stack: 99,
            tablet_rune: None,
            value: Some(10),
            icon_path: Some("icons/items/crystal_shard.png".into()),
        },
        ItemDef {
            id: "rune_tablet_blank".into(),
            name: "Blank Rune Tablet".into(),
            description: "An unmarked rune tablet, ready for inscription.".into(),
            kind: RuneTablet,
            max_stack: 1,
            tablet_rune: None,
            value: Some(20),
            icon_path: Some("icons/items/rune_tablet_blank.png".into()),
        },
        ItemDef {
            id: "rune_tablet_fire".into(),
            name: "Rune Tablet of Fire".into(),
            description: "A rune tablet inscribed with the Fire rune.".into(),
            kind: RuneTablet,
            max_stack: 1,
            tablet_rune: Some(arcane_core::Id64::from_str("fire")),
            value: Some(50),
            icon_path: Some("icons/items/rune_tablet_fire.png".into()),
        },
        ItemDef {
            id: "mana_potion_minor".into(),
            name: "Minor Mana Potion".into(),
            description: "Restores 25 mana when consumed.".into(),
            kind: Consumable,
            max_stack: 20,
            tablet_rune: None,
            value: Some(15),
            icon_path: Some("icons/items/mana_potion_minor.png".into()),
        },
        ItemDef {
            id: "arcanist_journal".into(),
            name: "Arcanist's Journal".into(),
            description: "A weathered journal. Cannot be discarded.".into(),
            kind: QuestItem,
            max_stack: 1,
            tablet_rune: None,
            value: None,
            icon_path: Some("icons/items/journal.png".into()),
        },
        ItemDef {
            id: "decayed_chisel".into(),
            name: "Decayed Chisel".into(),
            description: "A chisel whose edge crumbles to dust at the touch.".into(),
            kind: DecayedTool,
            max_stack: 1,
            tablet_rune: None,
            value: None,
            icon_path: Some("icons/items/decayed_chisel.png".into()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_registry_default_has_expected_items() {
        let reg = default_item_registry();
        assert!(reg.get_by_str("wood").is_some());
        assert!(reg.get_by_str("mana_dust").is_some());
        assert!(reg.get_by_str("rune_tablet_fire").is_some());
        assert!(reg.get_by_str("arcanist_journal").is_some());
    }

    #[test]
    fn item_def_stable_id_matches_string() {
        let reg = default_item_registry();
        let wood = reg.get_by_str("wood").unwrap();
        assert_eq!(wood.stable_id(), Id64::from_str("wood"));
    }

    #[test]
    fn rune_tablet_does_not_stack() {
        let reg = default_item_registry();
        let tablet = reg.get_by_str("rune_tablet_blank").unwrap();
        assert_eq!(tablet.max_stack, 1);
        assert!(!tablet.is_stackable());
    }

    #[test]
    fn inventory_add_creates_new_slot_when_empty() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 5, &reg);
        assert_eq!(inv.count_of(wood), 5);
        assert_eq!(inv.slot_count(), 1);
    }

    #[test]
    fn inventory_add_stacks_into_existing_slot() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 5, &reg);
        inv.add(wood, 3, &reg);
        assert_eq!(inv.count_of(wood), 8);
        assert_eq!(inv.slot_count(), 1, "should stack into same slot");
    }

    #[test]
    fn inventory_add_overflow_opens_new_slot() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        // Wood max_stack = 99.
        inv.add(wood, 100, &reg);
        assert_eq!(inv.count_of(wood), 100);
        assert_eq!(inv.slot_count(), 2, "100 items with max 99 should split into 2 slots");
    }

    #[test]
    fn inventory_remove_reduces_count() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 10, &reg);
        let removed = inv.remove(wood, 4);
        assert_eq!(removed, 4);
        assert_eq!(inv.count_of(wood), 6);
    }

    #[test]
    fn inventory_remove_more_than_available_returns_actual() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 5, &reg);
        let removed = inv.remove(wood, 10);
        assert_eq!(removed, 5);
        assert_eq!(inv.count_of(wood), 0);
    }

    #[test]
    fn inventory_remove_clears_empty_slot() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 5, &reg);
        inv.remove(wood, 5);
        assert_eq!(inv.slot_count(), 0);
    }

    #[test]
    fn inventory_has_check() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        inv.add(wood, 10, &reg);
        assert!(inv.has(wood, 10));
        assert!(inv.has(wood, 5));
        assert!(!inv.has(wood, 11));
    }

    #[test]
    fn inventory_separates_items_by_type() {
        let reg = default_item_registry();
        let mut inv = Inventory::new();
        let wood = reg.get_by_str("wood").unwrap().stable_id();
        let stone = reg.get_by_str("stone").unwrap().stable_id();
        inv.add(wood, 10, &reg);
        inv.add(stone, 20, &reg);
        inv.add(wood, 5, &reg);
        assert_eq!(inv.count_of(wood), 15);
        assert_eq!(inv.count_of(stone), 20);
        // Two distinct item types in two slots (wood stacked, stone stacked).
        assert_eq!(inv.slot_count(), 2);
    }

    #[test]
    fn item_def_ron_roundtrip() {
        let item = default_items().into_iter().find(|i| i.id == "mana_dust").unwrap();
        let s = ron::to_string(&item).unwrap();
        let back: ItemDef = ron::from_str(&s).unwrap();
        assert_eq!(back.id, "mana_dust");
        assert_eq!(back.kind, ItemKind::Material);
        assert_eq!(back.max_stack, 999);
    }

    #[test]
    fn item_def_postcard_roundtrip() {
        let item = default_items().into_iter().find(|i| i.id == "rune_tablet_fire").unwrap();
        let bytes = postcard::to_allocvec(&item).unwrap();
        let back: ItemDef = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.id, "rune_tablet_fire");
        assert_eq!(back.kind, ItemKind::RuneTablet);
        assert_eq!(back.tablet_rune, Some(Id64::from_str("fire")));
    }
}
