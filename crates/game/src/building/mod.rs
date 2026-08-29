//! Building system — "stabilizing reality," not just placing walls.
//!
//! Per the design doc:
//!   "Building should feel like stabilizing reality, not merely placing
//!    prefabricated walls."
//!
//! Each placed structure has:
//!   - a position
//!   - a footprint (cell-aligned to the chunk grid)
//!   - a stability value that decays over time without maintenance
//!   - an effect on local mana density / corruption

use crate::corruption::CorruptionState;
use arcane_core::IdUlid;
use serde::{Deserialize, Serialize};

/// A structure archetype — data-driven definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureDef {
    /// Stable string id — e.g. "ward_pylon", "storage_cache".
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Footprint in chunk-voxel cells (x, y, z).
    pub footprint: [u32; 3],
    /// Build cost in mana dust units.
    pub mana_dust_cost: u32,
    /// Build cost in crystal shards.
    pub crystal_cost: u32,
    /// Base stability — decays at this rate per second.
    pub stability_decay_per_sec: f32,
    /// Effect this structure has on local corruption (per sec).
    pub corruption_delta_per_sec: f32,
    /// Effect this structure has on local mana density (per sec).
    pub mana_delta_per_sec: f32,
    /// True if this structure provides sanctuary effect (warm, stable).
    pub provides_sanctuary: bool,
}

/// A placed structure instance in the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    /// Runtime id.
    pub id: IdUlid,
    /// Stable def id.
    pub def_id: String,
    /// Position in world space (anchor = min corner of footprint).
    pub position: [f32; 3],
    /// Current stability (0.0..1.0). When this hits 0, the structure collapses.
    pub stability: f32,
    /// Rotation (Y axis, in 90-degree increments: 0..3).
    pub rotation_y_90: u8,
}

impl Structure {
    /// Constructs a new structure from a def at the given position.
    pub fn new(def: &StructureDef, position: [f32; 3]) -> Self {
        Self {
            id: IdUlid::new(),
            def_id: def.id.clone(),
            position,
            stability: 1.0,
            rotation_y_90: 0,
        }
    }

    /// Returns the AABB of this structure's footprint in world space.
    pub fn world_aabb(&self, def: &StructureDef) -> ([f32; 3], [f32; 3]) {
        let min = self.position;
        let max = [
            self.position[0] + def.footprint[0] as f32,
            self.position[1] + def.footprint[1] as f32,
            self.position[2] + def.footprint[2] as f32,
        ];
        (min, max)
    }

    /// True if this structure is collapsed (stability <= 0).
    pub fn is_collapsed(&self) -> bool {
        self.stability <= 0.0
    }

    /// Advances the structure's stability decay. Returns the new stability.
    pub fn tick(&mut self, dt: f32, def: &StructureDef) -> f32 {
        self.stability = (self.stability - def.stability_decay_per_sec * dt).max(0.0);
        self.stability
    }

    /// Repairs the structure by `amount` (clamped to 1.0).
    pub fn repair(&mut self, amount: f32) {
        self.stability = (self.stability + amount).min(1.0);
    }
}

/// Validates whether a structure can be placed at the given position.
/// Returns Ok(()) if valid, Err with reason if not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementError {
    /// Footprint extends outside the loaded world (not enough chunks loaded).
    OutsideLoadedArea,
    /// Footprint overlaps an existing structure.
    OverlapsExisting,
    /// Footprint intersects solid terrain (not enough clear space).
    InsufficientClearance,
    /// Player lacks the required resources.
    InsufficientResources,
}

/// Check if the player can afford to build `def`.
pub fn can_afford(def: &StructureDef, mana_dust: u32, crystal: u32) -> bool {
    mana_dust >= def.mana_dust_cost && crystal >= def.crystal_cost
}

/// Apply the structure's effects to the local corruption state. Call once
/// per tick (the structure's `tick` returns the new stability, which feeds
/// the corruption multiplier here).
pub fn apply_corruption_effect(
    structure: &Structure,
    def: &StructureDef,
    corruption: &mut CorruptionState,
    dt: f32,
) {
    if structure.is_collapsed() {
        return;
    }
    let stability_mult = structure.stability;
    let delta = def.corruption_delta_per_sec * stability_mult * dt;
    if delta < 0.0 {
        corruption.reduce(-delta);
    } else {
        corruption.add(delta);
    }
}

/// Default starter structures available to the player.
pub fn default_structures() -> Vec<StructureDef> {
    vec![
        StructureDef {
            id: "ward_pylon".into(),
            name: "Ward Pylon".into(),
            description: "A small pylon that dampens local corruption.".into(),
            footprint: [1, 2, 1],
            mana_dust_cost: 5,
            crystal_cost: 0,
            stability_decay_per_sec: 0.001,
            corruption_delta_per_sec: -0.005,
            mana_delta_per_sec: 0.001,
            provides_sanctuary: false,
        },
        StructureDef {
            id: "storage_cache".into(),
            name: "Storage Cache".into(),
            description: "A small container for hoarded resources.".into(),
            footprint: [1, 1, 1],
            mana_dust_cost: 2,
            crystal_cost: 0,
            stability_decay_per_sec: 0.0005,
            corruption_delta_per_sec: 0.0,
            mana_delta_per_sec: 0.0,
            provides_sanctuary: false,
        },
        StructureDef {
            id: "research_altar".into(),
            name: "Research Altar".into(),
            description: "A focal point for studying magical phenomena.".into(),
            footprint: [2, 1, 2],
            mana_dust_cost: 20,
            crystal_cost: 5,
            stability_decay_per_sec: 0.001,
            corruption_delta_per_sec: 0.0,
            mana_delta_per_sec: 0.005,
            provides_sanctuary: true,
        },
        StructureDef {
            id: "sanctuary_core".into(),
            name: "Sanctuary Core".into(),
            description: "Stabilizes a region, repels corruption, restores mana.".into(),
            footprint: [3, 3, 3],
            mana_dust_cost: 100,
            crystal_cost: 20,
            stability_decay_per_sec: 0.0002,
            corruption_delta_per_sec: -0.02,
            mana_delta_per_sec: 0.02,
            provides_sanctuary: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corruption::CorruptionState;

    #[test]
    fn structure_starts_at_full_stability() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let s = Structure::new(&def, [0.0, 0.0, 0.0]);
        assert!((s.stability - 1.0).abs() < 1e-6);
        assert!(!s.is_collapsed());
    }

    #[test]
    fn structure_decay_reduces_stability() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let mut s = Structure::new(&def, [0.0, 0.0, 0.0]);
        s.tick(10.0, &def);  // 0.001/sec * 10s = 0.01 decay
        assert!((s.stability - 0.99).abs() < 1e-6);
    }

    #[test]
    fn structure_collapses_when_stability_zero() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let mut s = Structure::new(&def, [0.0, 0.0, 0.0]);
        s.stability = 0.001;
        s.tick(10.0, &def);
        assert!(s.is_collapsed());
    }

    #[test]
    fn structure_repair_restores_stability() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let mut s = Structure::new(&def, [0.0, 0.0, 0.0]);
        s.stability = 0.5;
        s.repair(0.3);
        assert!((s.stability - 0.8).abs() < 1e-6);
        s.repair(1.0);
        assert!((s.stability - 1.0).abs() < 1e-6, "repair clamps to 1.0");
    }

    #[test]
    fn structure_world_aabb_uses_footprint() {
        let def = default_structures().into_iter().find(|d| d.id == "research_altar").unwrap();
        let s = Structure::new(&def, [10.0, 0.0, 5.0]);
        let (min, max) = s.world_aabb(&def);
        assert_eq!(min, [10.0, 0.0, 5.0]);
        assert_eq!(max, [12.0, 1.0, 7.0]); // 2x1x2 footprint
    }

    #[test]
    fn can_afford_checks_resources() {
        let def = default_structures().into_iter().find(|d| d.id == "research_altar").unwrap();
        assert!(!can_afford(&def, 5, 0));
        assert!(!can_afford(&def, 20, 4));
        assert!(can_afford(&def, 20, 5));
        assert!(can_afford(&def, 100, 10));
    }

    #[test]
    fn ward_pylon_reduces_local_corruption() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let s = Structure::new(&def, [0.0, 0.0, 0.0]);
        let mut c = CorruptionState::new(0.5);
        apply_corruption_effect(&s, &def, &mut c, 1.0);
        // corruption_delta_per_sec = -0.005, stability = 1.0, dt = 1.0
        // reduction = 0.005 * 1.0 * 1.0 = 0.005
        assert!((c.level - 0.495).abs() < 1e-4, "got {}", c.level);
    }

    #[test]
    fn collapsed_structure_has_no_effect() {
        let def = default_structures().into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let mut s = Structure::new(&def, [0.0, 0.0, 0.0]);
        s.stability = 0.0;
        let mut c = CorruptionState::new(0.5);
        apply_corruption_effect(&s, &def, &mut c, 1.0);
        assert!((c.level - 0.5).abs() < 1e-6, "collapsed should have no effect");
    }

    #[test]
    fn structure_postcard_roundtrip() {
        let def = default_structures().into_iter().find(|d| d.id == "storage_cache").unwrap();
        let s = Structure::new(&def, [1.0, 2.0, 3.0]);
        let bytes = postcard::to_allocvec(&s).unwrap();
        let back: Structure = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(s.def_id, back.def_id);
        assert_eq!(s.position, back.position);
        assert_eq!(s.stability, back.stability);
    }

    #[test]
    fn default_structures_have_expected_entries() {
        let defs = default_structures();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"ward_pylon"));
        assert!(ids.contains(&"storage_cache"));
        assert!(ids.contains(&"research_altar"));
        assert!(ids.contains(&"sanctuary_core"));
    }
}
