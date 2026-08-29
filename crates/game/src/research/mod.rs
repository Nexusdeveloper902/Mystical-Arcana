//! Research — unlocking runes, discovering schematics, upgrading capabilities.
//!
//! Per the design doc:
//!   "Research must affect actual gameplay."
//!   "Research tree: unlocking runes, discovering schematics, upgrading
//!    capabilities, consuming resources, consuming magical knowledge."

use arcane_core::Id64;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A research node — a discrete unlock in the research tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNode {
    /// Stable id (e.g. "rune_pierce", "spell_fire_bolt").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Required mana dust to complete.
    pub mana_dust_cost: u32,
    /// Required crystal shards.
    pub crystal_cost: u32,
    /// Required research progress (seconds).
    pub research_time_secs: f32,
    /// Prerequisite research node ids.
    pub prerequisites: Vec<String>,
    /// What this node unlocks (runes, schematics, capabilities).
    pub unlocks: Vec<ResearchUnlock>,
}

/// What a research node unlocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResearchUnlock {
    /// Unlocks a rune (by stable string id).
    Rune(String),
    /// Unlocks a spell schematic (by stable string id).
    Schematic(String),
    /// Increases max mana by the given amount.
    MaxMana(f32),
    /// Increases base mana regen by the given amount.
    ManaRegen(f32),
    /// Increases the burn threshold.
    BurnThreshold(f32),
    /// Custom capability flag.
    Capability(String),
}

/// A single active research project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveResearch {
    /// The node id being researched.
    pub node_id: String,
    /// Elapsed research time in seconds.
    pub elapsed_secs: f32,
}

/// Player's research state.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    /// Set of completed node ids.
    pub completed: HashSet<String>,
    /// Currently active research project, if any.
    pub active: Option<ActiveResearch>,
}

impl ResearchState {
    /// Empty research state.
    pub fn new() -> Self {
        Self::default()
    }

    /// True if the player has completed the given node.
    pub fn has_completed(&self, node_id: &str) -> bool {
        self.completed.contains(node_id)
    }

    /// Returns true if the player can start researching the given node:
    /// - all prerequisites are completed
    /// - not already completed
    /// - no active research in progress
    pub fn can_start(&self, node: &ResearchNode) -> bool {
        if self.completed.contains(&node.id) {
            return false;
        }
        if self.active.is_some() {
            return false;
        }
        for pre in &node.prerequisites {
            if !self.completed.contains(pre) {
                return false;
            }
        }
        true
    }

    /// Starts researching `node`. Returns Err if cannot start.
    pub fn start(&mut self, node: &ResearchNode) -> Result<(), &'static str> {
        if !self.can_start(node) {
            return Err("cannot start research");
        }
        self.active = Some(ActiveResearch { node_id: node.id.clone(), elapsed_secs: 0.0 });
        Ok(())
    }

    /// Advances the active research by `dt` seconds. Returns Some(unlocks) if
    /// research completed this tick, None otherwise.
    pub fn tick<'a>(&mut self, dt: f32, node: &'a ResearchNode) -> Option<&'a [ResearchUnlock]> {
        let active = self.active.as_mut()?;
        if active.node_id != node.id {
            return None;
        }
        active.elapsed_secs += dt;
        if active.elapsed_secs >= node.research_time_secs {
            self.completed.insert(node.id.clone());
            self.active = None;
            return Some(&node.unlocks);
        }
        None
    }

    /// Cancels the active research.
    pub fn cancel(&mut self) {
        self.active = None;
    }

    /// Returns all completed node ids.
    pub fn completed_nodes(&self) -> &HashSet<String> {
        &self.completed
    }
}

/// Default starter research tree.
pub fn default_research_tree() -> Vec<ResearchNode> {
    vec![
        ResearchNode {
            id: "first_gather".into(),
            name: "Gather Mastery".into(),
            description: "Master the Gather rune.".into(),
            mana_dust_cost: 5,
            crystal_cost: 0,
            research_time_secs: 10.0,
            prerequisites: vec![],
            unlocks: vec![ResearchUnlock::Rune("gather".into())],
        },
        ResearchNode {
            id: "pierce_unlock".into(),
            name: "Pierce Insight".into(),
            description: "Discover the Pierce rune.".into(),
            mana_dust_cost: 10,
            crystal_cost: 0,
            research_time_secs: 20.0,
            prerequisites: vec!["first_gather".into()],
            unlocks: vec![ResearchUnlock::Rune("pierce".into())],
        },
        ResearchNode {
            id: "fire_bolt_research".into(),
            name: "Fire Bolt Theory".into(),
            description: "Discover the Fire Bolt schematic.".into(),
            mana_dust_cost: 15,
            crystal_cost: 2,
            research_time_secs: 30.0,
            prerequisites: vec!["pierce_unlock".into()],
            unlocks: vec![
                ResearchUnlock::Schematic("fire_bolt".into()),
                ResearchUnlock::Rune("fire".into()),
            ],
        },
        ResearchNode {
            id: "mana_capacity_1".into(),
            name: "Mana Capacity I".into(),
            description: "Increase maximum mana by 25.".into(),
            mana_dust_cost: 20,
            crystal_cost: 1,
            research_time_secs: 25.0,
            prerequisites: vec!["first_gather".into()],
            unlocks: vec![ResearchUnlock::MaxMana(25.0)],
        },
        ResearchNode {
            id: "burn_resistance".into(),
            name: "Mana Burn Resistance".into(),
            description: "Increase burn threshold by 10.".into(),
            mana_dust_cost: 25,
            crystal_cost: 5,
            research_time_secs: 40.0,
            prerequisites: vec!["mana_capacity_1".into()],
            unlocks: vec![ResearchUnlock::BurnThreshold(10.0)],
        },
    ]
}

/// Look up a research node by id.
pub fn find_node<'a>(nodes: &'a [ResearchNode], id: &str) -> Option<&'a ResearchNode> {
    nodes.iter().find(|n| n.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_completes_nothing() {
        let s = ResearchState::new();
        assert!(!s.has_completed("anything"));
        assert!(s.active.is_none());
    }

    #[test]
    fn can_start_with_met_prerequisites() {
        let nodes = default_research_tree();
        let node = find_node(&nodes, "first_gather").unwrap();
        let mut s = ResearchState::new();
        assert!(s.can_start(node));
        s.start(node).unwrap();
        assert!(s.active.is_some());
    }

    #[test]
    fn cannot_start_with_unmet_prerequisites() {
        let nodes = default_research_tree();
        let fire_bolt = find_node(&nodes, "fire_bolt_research").unwrap();
        let mut s = ResearchState::new();
        assert!(!s.can_start(fire_bolt));
        assert!(s.start(fire_bolt).is_err());
    }

    #[test]
    fn cannot_start_when_active_research_exists() {
        let nodes = default_research_tree();
        let n1 = find_node(&nodes, "first_gather").unwrap();
        let n2 = find_node(&nodes, "mana_capacity_1").unwrap();
        let mut s = ResearchState::new();
        s.start(n1).unwrap();
        assert!(!s.can_start(n2));
    }

    #[test]
    fn tick_progresses_and_completes() {
        let nodes = default_research_tree();
        let node = find_node(&nodes, "first_gather").unwrap();
        let mut s = ResearchState::new();
        s.start(node).unwrap();
        // node.research_time_secs = 10.0
        assert!(s.tick(5.0, node).is_none());
        assert!(s.active.is_some(), "should still be active");
        let unlocks = s.tick(5.0, node);
        assert!(unlocks.is_some(), "should complete on second tick");
        assert!(s.has_completed("first_gather"));
        assert!(s.active.is_none());
    }

    #[test]
    fn cancel_aborts_active_research() {
        let nodes = default_research_tree();
        let node = find_node(&nodes, "first_gather").unwrap();
        let mut s = ResearchState::new();
        s.start(node).unwrap();
        s.cancel();
        assert!(s.active.is_none());
        assert!(!s.has_completed("first_gather"));
    }

    #[test]
    fn prerequisites_chain_correctly() {
        let nodes = default_research_tree();
        let mut s = ResearchState::new();
        let n1 = find_node(&nodes, "first_gather").unwrap();
        s.start(n1).unwrap();
        for _ in 0..100 {
            if s.tick(1.0, n1).is_some() {
                break;
            }
        }
        assert!(s.has_completed("first_gather"));
        // After first_gather completes, pierce_unlock should be available.
        let n2 = find_node(&nodes, "pierce_unlock").unwrap();
        assert!(s.can_start(n2));
    }

    #[test]
    fn research_unlocks_provide_correct_data() {
        let nodes = default_research_tree();
        let fire_bolt = find_node(&nodes, "fire_bolt_research").unwrap();
        let unlocks = &fire_bolt.unlocks;
        assert!(unlocks.iter().any(|u| matches!(u, ResearchUnlock::Schematic(s) if s == "fire_bolt")));
        assert!(unlocks.iter().any(|u| matches!(u, ResearchUnlock::Rune(r) if r == "fire")));
    }

    #[test]
    fn research_state_postcard_roundtrip() {
        let mut s = ResearchState::new();
        s.completed.insert("a".into());
        s.completed.insert("b".into());
        let bytes = postcard::to_allocvec(&s).unwrap();
        let back: ResearchState = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(s.completed, back.completed);
    }
}
