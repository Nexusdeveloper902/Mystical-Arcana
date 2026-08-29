//! Data-driven rune system.
//!
//! Per the design doc, runes are:
//! - The language of magic. Five core categories: Movement, Transformation,
//!   Protection, Destruction, Manipulation.
//! - Greek-inspired written language — a coherent visual system.
//! - Data-driven: definitions live in `Assets/data/runes/` as RON files.
//! - Compositional: combinations produce Schematics (e.g., Fire + Pierce
//!   → Fire Bolt).
//! - Share identity across UI, world, tablets, research, spells, VFX.
//!
//! All rune logic is pure-data and unit-testable headless. The visual glyph
//! is a separate `RuneGlyph` enum used by the UI and VFX systems.

use arcane_core::Id64;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One of the five core magical categories. Each rune belongs to exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RuneCategory {
    /// Movement — motion, displacement, momentum, traversal.
    Movement = 0,
    /// Transformation — change of state, form, or substance.
    Transformation = 1,
    /// Protection — warding, shielding, stabilization.
    Protection = 2,
    /// Destruction — breaking, severing, unbinding.
    Destruction = 3,
    /// Manipulation — controlling, lifting, redirecting, gathering.
    Manipulation = 4,
}

impl RuneCategory {
    /// Returns the canonical short name used in data files.
    pub fn as_str(self) -> &'static str {
        match self {
            RuneCategory::Movement => "movement",
            RuneCategory::Transformation => "transformation",
            RuneCategory::Protection => "protection",
            RuneCategory::Destruction => "destruction",
            RuneCategory::Manipulation => "manipulation",
        }
    }

    /// Returns the canonical Greek-inspired symbol used in the visual glyph
    /// language. These are real Greek letters chosen for visual coherence.
    pub fn greek_letter(self) -> &'static str {
        match self {
            RuneCategory::Movement => "Μ",    // Mu — motion
            RuneCategory::Transformation => "Τ",  // Tau — transformation
            RuneCategory::Protection => "Π",  // Pi — protection (like a shield)
            RuneCategory::Destruction => "Δ", // Delta — destruction (like a blade)
            RuneCategory::Manipulation => "Ψ", // Psi — manipulation (mind/hand)
        }
    }

    /// Parses from the canonical short name.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "movement" => Some(Self::Movement),
            "transformation" => Some(Self::Transformation),
            "protection" => Some(Self::Protection),
            "destruction" => Some(Self::Destruction),
            "manipulation" => Some(Self::Manipulation),
            _ => None,
        }
    }
}

/// Visual glyph for a rune — the canonical on-screen shape. Stored separately
/// from the data definition so the renderer/UI can decide how to draw.
///
/// Each glyph is a known primitive drawn procedurally; the game does not
/// depend on any external font for runes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum RuneGlyph {
    /// A circle with a vertical line through it (e.g. Movement base).
    CircleVertical = 0,
    /// A triangle pointing up (e.g. basic Fire).
    TriangleUp = 1,
    /// A triangle pointing down.
    TriangleDown = 2,
    /// A horizontal line through a circle.
    CircleHorizontal = 3,
    /// A square with a diagonal cross.
    SquareCross = 4,
    /// A spiral (single curve).
    Spiral = 5,
    /// A vertical line with two horizontal serifs.
    LineWithSerifs = 6,
    /// A wave (three sinusoidal humps).
    Wave = 7,
    /// A six-pointed star (compound of two triangles).
    HexStar = 8,
}

/// A complete rune definition. Lives in `Assets/data/runes/*.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneDef {
    /// Stable string identifier — e.g. `"fire"`, `"pierce"`, `"gather"`.
    pub id: String,
    /// Display name shown to the player.
    pub name: String,
    /// One of the five core categories.
    pub category: RuneCategory,
    /// Human-readable description.
    pub description: String,
    /// The canonical glyph used to draw this rune.
    pub glyph: RuneGlyph,
    /// Mana cost modifier — multiplier on a spell's base cost when this rune
    /// is the primary.
    pub mana_modifier: f32,
    /// Power modifier — multiplier on the spell's effect strength.
    pub power_modifier: f32,
    /// Cooldown modifier (seconds added to the base cooldown).
    pub cooldown_modifier: f32,
    /// Optional behavioral tag — recognized by the spell engine to dispatch
    /// to a specific spell effect.
    pub behavior: Option<String>,
}

impl RuneDef {
    /// Computes the stable `Id64` for this rune from its `id` string.
    pub fn stable_id(&self) -> Id64 {
        Id64::from_str(&self.id)
    }
}

/// The full registry of all runes known to the game. Loaded from
/// `Assets/data/runes/*.ron` at startup.
#[derive(Debug, Default)]
pub struct RuneRegistry {
    runes: HashMap<Id64, RuneDef>,
    /// Lookup by string id for the data editor.
    by_string: HashMap<String, Id64>,
}

impl RuneRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a rune definition.
    pub fn register(&mut self, def: RuneDef) {
        let id = def.stable_id();
        self.by_string.insert(def.id.clone(), id);
        self.runes.insert(id, def);
    }

    /// Looks up a rune by its stable ID.
    pub fn get(&self, id: Id64) -> Option<&RuneDef> {
        self.runes.get(&id)
    }

    /// Looks up a rune by its string id.
    pub fn get_by_str(&self, s: &str) -> Option<&RuneDef> {
        self.by_string.get(s).and_then(|id| self.runes.get(id))
    }

    /// Number of registered runes.
    pub fn len(&self) -> usize {
        self.runes.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.runes.is_empty()
    }

    /// Iterates all registered runes.
    pub fn iter(&self) -> impl Iterator<Item = (&Id64, &RuneDef)> {
        self.runes.iter()
    }

    /// Returns all runes in a given category.
    pub fn by_category(&self, cat: RuneCategory) -> Vec<&RuneDef> {
        self.runes.values().filter(|r| r.category == cat).collect()
    }
}

/// A pair of runes composed together. Used by the Schematic system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunePair {
    /// First rune's stable ID. Convention: the verb (action).
    pub primary: Id64,
    /// Second rune's stable ID. Convention: the noun (target).
    pub secondary: Id64,
}

impl RunePair {
    /// Constructs a pair from two rune IDs.
    pub const fn new(primary: Id64, secondary: Id64) -> Self {
        Self { primary, secondary }
    }

    /// Constructs from two string identifiers (looked up against the registry).
    pub fn from_strings(reg: &RuneRegistry, a: &str, b: &str) -> Option<Self> {
        Some(Self::new(
            reg.get_by_str(a)?.stable_id(),
            reg.get_by_str(b)?.stable_id(),
        ))
    }

    /// Returns the pair in canonical (sorted) order so that
    /// (A, B) and (B, A) hash to the same key.
    pub fn canonical(self) -> Self {
        if self.primary.as_u64() <= self.secondary.as_u64() {
            self
        } else {
            Self::new(self.secondary, self.primary)
        }
    }
}

/// Provides a default set of starter runes. Useful for tests and for
/// bootstrapping the game before art content lands.
pub fn default_rune_set() -> Vec<RuneDef> {
    use RuneCategory::*;
    use RuneGlyph::*;
    vec![
        RuneDef {
            id: "gather".into(),
            name: "Gather".into(),
            category: Manipulation,
            description: "Pulls loose matter toward the caster.".into(),
            glyph: CircleVertical,
            mana_modifier: 0.6,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: Some("gather_bolt".into()),
        },
        RuneDef {
            id: "pierce".into(),
            name: "Pierce".into(),
            category: Destruction,
            description: "Concentrates force into a single penetrating ray.".into(),
            glyph: TriangleUp,
            mana_modifier: 1.2,
            power_modifier: 1.5,
            cooldown_modifier: 0.5,
            behavior: Some("pierce".into()),
        },
        RuneDef {
            id: "fire".into(),
            name: "Fire".into(),
            category: Transformation,
            description: "Transforms ambient mana into heat and light.".into(),
            glyph: TriangleUp,
            mana_modifier: 1.0,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: Some("fire".into()),
        },
        RuneDef {
            id: "ice".into(),
            name: "Ice".into(),
            category: Transformation,
            description: "Crystallizes ambient mana into freezing cold.".into(),
            glyph: SquareCross,
            mana_modifier: 1.0,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: Some("ice".into()),
        },
        RuneDef {
            id: "ward".into(),
            name: "Ward".into(),
            category: Protection,
            description: "Stabilizes a small region, damping hostile forces.".into(),
            glyph: SquareCross,
            mana_modifier: 1.4,
            power_modifier: 1.0,
            cooldown_modifier: 1.0,
            behavior: Some("ward".into()),
        },
        RuneDef {
            id: "leap".into(),
            name: "Leap".into(),
            category: Movement,
            description: "Displaces the caster rapidly over a short distance.".into(),
            glyph: CircleHorizontal,
            mana_modifier: 0.8,
            power_modifier: 1.0,
            cooldown_modifier: 1.5,
            behavior: Some("leap".into()),
        },
        RuneDef {
            id: "bind".into(),
            name: "Bind".into(),
            category: Manipulation,
            description: "Holds matter in place, refusing to move.".into(),
            glyph: LineWithSerifs,
            mana_modifier: 0.9,
            power_modifier: 1.0,
            cooldown_modifier: 0.3,
            behavior: Some("bind".into()),
        },
        RuneDef {
            id: "resonance".into(),
            name: "Resonance".into(),
            category: Manipulation,
            description: "Tunes the caster to nearby mana, boosting regeneration.".into(),
            glyph: Spiral,
            mana_modifier: 0.5,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: Some("resonance".into()),
        },
        RuneDef {
            id: "shatter".into(),
            name: "Shatter".into(),
            category: Destruction,
            description: "Severs the structure of matter at a focal point.".into(),
            glyph: TriangleDown,
            mana_modifier: 1.5,
            power_modifier: 1.8,
            cooldown_modifier: 1.0,
            behavior: Some("shatter".into()),
        },
        RuneDef {
            id: "flow".into(),
            name: "Flow".into(),
            category: Movement,
            description: "Directs the flow of mana or matter along a path.".into(),
            glyph: Wave,
            mana_modifier: 0.7,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: Some("flow".into()),
        },
    ]
}

/// Convenience: builds a default registry with all starter runes.
pub fn default_registry() -> RuneRegistry {
    let mut reg = RuneRegistry::new();
    for r in default_rune_set() {
        reg.register(r);
    }
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rune_categories_have_canonical_names() {
        assert_eq!(RuneCategory::Movement.as_str(), "movement");
        assert_eq!(RuneCategory::Transformation.as_str(), "transformation");
        assert_eq!(RuneCategory::Protection.as_str(), "protection");
        assert_eq!(RuneCategory::Destruction.as_str(), "destruction");
        assert_eq!(RuneCategory::Manipulation.as_str(), "manipulation");
    }

    #[test]
    fn rune_categories_round_trip_via_string() {
        for cat in [
            RuneCategory::Movement,
            RuneCategory::Transformation,
            RuneCategory::Protection,
            RuneCategory::Destruction,
            RuneCategory::Manipulation,
        ] {
            let s = cat.as_str();
            assert_eq!(RuneCategory::from_str(s), Some(cat));
        }
        assert_eq!(RuneCategory::from_str("nonsense"), None);
    }

    #[test]
    fn rune_categories_have_distinct_greek_letters() {
        let letters: Vec<&str> = [
            RuneCategory::Movement,
            RuneCategory::Transformation,
            RuneCategory::Protection,
            RuneCategory::Destruction,
            RuneCategory::Manipulation,
        ].iter().map(|c| c.greek_letter()).collect();
        let unique: std::collections::HashSet<_> = letters.iter().cloned().collect();
        assert_eq!(unique.len(), 5, "each category must have a distinct Greek letter");
    }

    #[test]
    fn default_registry_has_expected_runes() {
        let reg = default_registry();
        assert!(reg.len() >= 10);
        assert!(reg.get_by_str("fire").is_some());
        assert!(reg.get_by_str("pierce").is_some());
        assert!(reg.get_by_str("gather").is_some());
    }

    #[test]
    fn rune_def_stable_id_matches_string_id() {
        let reg = default_registry();
        let fire = reg.get_by_str("fire").unwrap();
        let sid = fire.stable_id();
        assert_eq!(sid, Id64::from_str("fire"));
    }

    #[test]
    fn rune_pair_canonical_order_is_stable() {
        let reg = default_registry();
        let fire = reg.get_by_str("fire").unwrap().stable_id();
        let pierce = reg.get_by_str("pierce").unwrap().stable_id();
        let p1 = RunePair::new(fire, pierce);
        let p2 = RunePair::new(pierce, fire);
        assert_eq!(p1.canonical(), p2.canonical(), "canonical form must be order-independent");
    }

    #[test]
    fn rune_pair_from_strings_resolves_via_registry() {
        let reg = default_registry();
        let p = RunePair::from_strings(&reg, "fire", "pierce");
        assert!(p.is_some());
        let p = p.unwrap();
        assert_eq!(p.primary, reg.get_by_str("fire").unwrap().stable_id());
        assert_eq!(p.secondary, reg.get_by_str("pierce").unwrap().stable_id());
    }

    #[test]
    fn rune_pair_from_strings_returns_none_for_unknown_runes() {
        let reg = default_registry();
        assert!(RunePair::from_strings(&reg, "fire", "nonexistent").is_none());
    }

    #[test]
    fn rune_registry_by_category_filters_correctly() {
        let reg = default_registry();
        let fire_runes = reg.by_category(RuneCategory::Transformation);
        let has_fire = fire_runes.iter().any(|r| r.id == "fire");
        let has_ice = fire_runes.iter().any(|r| r.id == "ice");
        let has_gather = fire_runes.iter().any(|r| r.id == "gather");
        assert!(has_fire, "fire should be Transformation");
        assert!(has_ice, "ice should be Transformation");
        assert!(!has_gather, "gather is Manipulation, not Transformation");
    }

    #[test]
    fn rune_def_serde_roundtrip_via_ron() {
        let r = default_rune_set().into_iter().find(|r| r.id == "fire").unwrap();
        let s = ron::to_string(&r).unwrap();
        let back: RuneDef = ron::from_str(&s).unwrap();
        assert_eq!(back.id, "fire");
        assert_eq!(back.category, RuneCategory::Transformation);
        assert_eq!(back.glyph, RuneGlyph::TriangleUp);
    }

    #[test]
    fn rune_def_postcard_roundtrip() {
        let r = default_rune_set().into_iter().find(|r| r.id == "ward").unwrap();
        let bytes = postcard::to_allocvec(&r).unwrap();
        let back: RuneDef = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.id, "ward");
        assert_eq!(back.category, RuneCategory::Protection);
    }

    #[test]
    fn rune_registry_lookup_by_id_and_string() {
        let mut reg = RuneRegistry::new();
        let r = RuneDef {
            id: "test_rune".into(),
            name: "Test".into(),
            category: RuneCategory::Movement,
            description: "".into(),
            glyph: RuneGlyph::Spiral,
            mana_modifier: 1.0,
            power_modifier: 1.0,
            cooldown_modifier: 0.0,
            behavior: None,
        };
        reg.register(r);
        let sid = Id64::from_str("test_rune");
        assert!(reg.get(sid).is_some());
        assert!(reg.get_by_str("test_rune").is_some());
        assert!(reg.get_by_str("missing").is_none());
    }

    #[test]
    fn rune_glyphs_are_serializable_as_strings() {
        // Ensure glyph enum serializes (default derives use number IDs).
        let g = RuneGlyph::HexStar;
        let s = serde_json::to_string(&g).unwrap();
        let back: RuneGlyph = serde_json::from_str(&s).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn full_default_set_serializes_as_ron_collection() {
        let set = default_rune_set();
        let s = ron::ser::to_string_pretty(
            &set,
            ron::ser::PrettyConfig::default(),
        ).unwrap();
        let back: Vec<RuneDef> = ron::from_str(&s).unwrap();
        assert_eq!(back.len(), set.len());
        assert_eq!(back[0].id, "gather");
    }
}
