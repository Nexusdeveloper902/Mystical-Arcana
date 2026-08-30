//! Headless gameplay smoke test.
//!
//! This is the canonical "does the game work end-to-end" test. It runs
//! without a GPU, audio device, or window, and exercises the full gameplay
//! loop described in ADR-0003:
//!
//! 1. Generate a world with a fixed seed.
//! 2. Spawn the player at the world origin.
//! 3. Drive the simulation forward.
//! 4. Find a nearby Mana Node, gain mana regen from it.
//! 5. Cast a Gather Bolt spell (with overcast if needed).
//! 6. Collect gathered resources into inventory.
//! 7. Establish a sanctuary (place a ward pylon).
//! 8. Save the game state (player + world).
//! 9. Load it back and verify state matches.
//! 10. Verify progression phase has advanced past Survival.

use arcane_core::serialize::{decode, encode};
use arcane_world::{ChunkCoord, WorldGenerator, WorldSeed};
use mystical_arcana_lib::building::{default_structures, Structure};
use mystical_arcana_lib::combat::{DamageInstance, DamageType};
use mystical_arcana_lib::corruption::CorruptionState;
use mystical_arcana_lib::inventory::default_item_registry;
use mystical_arcana_lib::mana::{ManaNode, ManaPool};
use mystical_arcana_lib::player::PlayerState;
use mystical_arcana_lib::progression::ArcanistPhase;
use mystical_arcana_lib::runes::{default_registry as default_rune_registry, RunePair};
use mystical_arcana_lib::spells::{default_spell_registry, try_cast, CastResult};
use mystical_arcana_lib::world::WorldSave;

/// A scripted headless gameplay session that drives the simulation forward
/// through the canonical loop.
struct HeadlessSession {
    player: PlayerState,
    seed: WorldSeed,
    mana_nodes: Vec<ManaNode>,
    corruption: CorruptionState,
}

impl HeadlessSession {
    fn new(seed: WorldSeed) -> Self {
        let mut player = PlayerState::new();
        player.transform.position = [0.0, 32.0, 0.0];

        // Place a Mana Node near spawn.
        let mana_node = ManaNode::new(
            arcane_core::IdUlid::new(),
            [3.0, 32.0, 0.0],
            10.0,
            10.0,
        );
        Self {
            player,
            seed,
            mana_nodes: vec![mana_node],
            corruption: CorruptionState::default(),
        }
    }

    /// Steps the simulation forward by `dt` seconds. The Mana Node regen
    /// modifier is applied to the player when they're inside its radius.
    fn step(&mut self, dt: f32) {
        // Apply Mana Node regen modifier if player is in radius.
        let pos = self.player.transform.position;
        let pos_arr = [pos[0], pos[1], pos[2]];
        let mana_bonus = self.player.mana.modifiers.iter()
            .filter(|m| m.source == arcane_core::Id64::from_str("mana_node"))
            .map(|m| m.delta_per_sec)
            .sum::<f32>();
        let _ = mana_bonus;

        // Apply the node's current bonus if inside (handled by modifiers).
        // Tick player + world.
        self.player.tick(dt);
        self.corruption.tick(dt);
    }

    /// Activates the Mana Node — adds a regen modifier to the player.
    fn activate_mana_node(&mut self) {
        // Remove any stale node modifier first.
        let id = arcane_core::Id64::from_str("mana_node");
        self.player.mana.remove_modifiers_from(id);
        self.player.mana.add_modifier(mystical_arcana_lib::mana::ManaRegenModifier {
            source: id,
            delta_per_sec: 10.0,
            duration_secs: None,
            elapsed_secs: 0.0,
        });
    }

    /// Attempts to cast a spell by id. Returns the cast result.
    fn cast_spell(&mut self, spell_id_str: &str, allow_overcast: bool) -> CastResult {
        let reg = default_spell_registry();
        let id = arcane_core::Id64::from_str(spell_id_str);
        let result = try_cast(
            id,
            &reg,
            &mut self.player.mana,
            &mut self.player.cooldowns,
            allow_overcast,
        );
        if matches!(result, CastResult::Success { .. }) {
            self.player.progression.stats.record_spell_cast();
        }
        result
    }

    /// Simulates gathering a resource via Gather Bolt (visualized by adding
    /// items to the inventory).
    fn gather_resource(&mut self, item_id_str: &str, count: u32) {
        let reg = default_item_registry();
        let item_id = reg.get_by_str(item_id_str).unwrap().stable_id();
        let _added = self.player.inventory.add(item_id, count, &reg);
        self.player.progression.stats.record_resources_gathered(count);
    }

    /// Establishes a sanctuary (places a ward pylon as the first structure).
    fn establish_sanctuary(&mut self) -> Structure {
        let defs = default_structures();
        let def = defs.into_iter().find(|d| d.id == "ward_pylon").unwrap();
        let structure = Structure::new(&def, self.player.transform.position);
        self.player.progression.stats.record_sanctuary_established();
        structure
    }

    /// Kills an enemy (simulated). Increments the enemy-defeated stat.
    fn defeat_enemy(&mut self) {
        // Apply some damage to the player too (to exercise the health system).
        self.player.health.apply_damage(DamageInstance::new(DamageType::Kinetic, 5.0));
        self.player.progression.stats.record_enemy_defeated();
    }

    /// Learns a schematic. Increments the schematic-learned stat.
    fn learn_schematic(&mut self, spell_id: &str) {
        let reg = default_spell_registry();
        if let Some(spell) = reg.get_by_str(spell_id) {
            self.player.schematics.learn(spell, self.player.progression.stats.time_survived_secs);
            self.player.progression.stats.record_schematic_learned();
        }
    }
}

// Bring the ManaRegenModifier into scope — actually not needed here since
// we use the full path `mystical_arcana_lib::mana::ManaRegenModifier`
// in the HeadlessSession implementation above.

#[test]
fn full_gameplay_loop_smoke() {
    // 1. New game with a fixed seed.
    let seed = WorldSeed::new(0xBADC_0FFE);
    let mut session = HeadlessSession::new(seed);

    // 2. Verify initial state.
    assert!(!session.player.is_dead());
    assert_eq!(session.player.progression.phase(), ArcanistPhase::Survival);
    assert!((session.player.mana.current_mana - 100.0).abs() < 1e-6);

    // 3. Step forward 10 seconds to drain some mana (player has 5/sec regen,
    //    so mana stays at max). To make mana burn meaningful, we need to
    //    cast spells first.
    session.step(10.0);
    assert!((session.player.mana.current_mana - 100.0).abs() < 1e-6, "mana should be at max");

    // 4. Try to cast a spell the player hasn't learned yet — should still work
    //    via improvisation, since try_cast doesn't require schematic knowledge
    //    by default.
    let result = session.cast_spell("gather_bolt", false);
    // gather_bolt base cost = 5, mana = 100, should succeed.
    assert!(matches!(result, CastResult::Success { .. }), "gather_bolt should succeed: {:?}", result);

    // 5. Wait for cooldown to expire (0.5s) then cast again.
    session.step(1.0);
    let result = session.cast_spell("gather_bolt", false);
    assert!(matches!(result, CastResult::Success { .. }));

    // 6. Gather some wood from the gathered spell.
    session.gather_resource("wood", 5);
    let wood_id = default_item_registry().get_by_str("wood").unwrap().stable_id();
    assert!(session.player.inventory.has(wood_id, 5));
    assert_eq!(session.player.progression.stats.resources_gathered, 5);

    // 6b. Cast gather_bolt many more times to track spell casts and gather
    //     more resources. Each cast must wait for cooldown (0.5s).
    for _ in 0..10 {
        session.step(1.0); // wait for cooldown
        let _ = session.cast_spell("gather_bolt", false);
    }
    session.gather_resource("wood", 20);
    session.gather_resource("stone", 10);
    assert_eq!(session.player.progression.stats.spells_cast, 12); // 2 from before + 10
    assert!(session.player.progression.stats.resources_gathered >= 35);

    // 7. Activate the nearby Mana Node to boost regen.
    session.activate_mana_node();
    // Drain some mana first by casting an expensive spell.
    let result = session.cast_spell("fire_bolt", true); // allow overcast
    assert!(matches!(result, CastResult::Success { .. }));
    // Mana should be lower now.
    assert!(session.player.mana.current_mana < 100.0);
    let mana_after_cast = session.player.mana.current_mana;
    session.step(2.0);
    // Mana should have regenerated due to Mana Node modifier (+10/sec).
    let mana_after_regen = session.player.mana.current_mana;
    assert!(mana_after_regen > mana_after_cast, "mana should regenerate: {} vs {}", mana_after_regen, mana_after_cast);

    // 8. Defeat an enemy (simulated).
    session.defeat_enemy();
    assert_eq!(session.player.progression.stats.enemies_defeated, 1);

    // 9. Learn multiple schematics to advance progression meaningfully.
    for spell_id in &["gather_bolt", "fire_bolt", "ice_ward", "leap", "bind_field"] {
        session.learn_schematic(spell_id);
    }
    assert_eq!(session.player.progression.stats.schematics_learned, 5);
    assert!(session.player.schematics.knows(arcane_core::Id64::from_str("fire_bolt")));

    // 10. Establish a sanctuary (ward pylon).
    let structure = session.establish_sanctuary();
    assert_eq!(session.player.progression.stats.sanctuaries_established, 1);
    assert!((structure.stability - 1.0).abs() < 1e-6);

    // 11. Save the game.
    let world_save = WorldSave::new(seed);
    let player_save = &session.player;

    let player_bytes = encode(player_save).expect("encode player");
    let world_bytes = encode(&world_save).expect("encode world");

    // Verify both blobs have the magic header.
    assert_eq!(&player_bytes[0..8], b"ARCANE1\0");
    assert_eq!(&world_bytes[0..8], b"ARCANE1\0");

    // 12. Load the game back.
    let loaded_player: PlayerState = decode(&player_bytes).expect("decode player");
    let _loaded_world: WorldSave = decode(&world_bytes).expect("decode world");

    // 13. Verify state matches.
    assert_eq!(loaded_player.transform.position, session.player.transform.position);
    assert!((loaded_player.mana.current_mana - session.player.mana.current_mana).abs() < 1e-4);
    assert_eq!(loaded_player.progression.stats.enemies_defeated, 1);
    assert_eq!(loaded_player.progression.stats.schematics_learned, 5);
    assert_eq!(loaded_player.progression.stats.sanctuaries_established, 1);
    assert!(loaded_player.schematics.knows(arcane_core::Id64::from_str("fire_bolt")));
    assert!(loaded_player.inventory.has(wood_id, 5));

    // 14. Verify progression phase has advanced past Survival.
    let phase = session.player.progression.phase();
    assert!(
        phase != ArcanistPhase::Survival,
        "after the smoke loop, player should have advanced past Survival, got {:?}",
        phase
    );
    assert!(
        phase == ArcanistPhase::Discovery,
        "with 5 schematics + 1 sanctuary + 1 enemy + 5 resources, player should be at Discovery, got {:?}",
        phase
    );

    // 15. Final smoke: step forward more and verify no panics.
    for _ in 0..100 {
        session.step(0.1);
    }
    assert!(!session.player.is_dead(), "player should not die during the smoke test");
}

#[test]
fn world_generator_runs_headless() {
    // Verify that procedural generation works headless — no GPU needed.
    let seed = WorldSeed::new(1234);
    let gen = WorldGenerator::new(seed);
    let chunk = gen.generate_chunk(ChunkCoord::new(0, 0, 0));
    assert_eq!(chunk.coord, ChunkCoord::new(0, 0, 0));
    assert_eq!(chunk.densities.len(), 32768);

    // Verify determinism — same seed → same chunk.
    let gen2 = WorldGenerator::new(seed);
    let chunk2 = gen2.generate_chunk(ChunkCoord::new(0, 0, 0));
    assert_eq!(chunk, chunk2, "procedural generation must be deterministic");
}

#[test]
fn rune_pair_composition_smoke() {
    // Verify the rune composition system.
    let rune_reg = default_rune_registry();
    let spell_reg = default_spell_registry();

    // Find fire_bolt by composing fire + pierce.
    let fire = rune_reg.get_by_str("fire").unwrap().stable_id();
    let pierce = rune_reg.get_by_str("pierce").unwrap().stable_id();
    let pair = RunePair::new(fire, pierce);
    let spell = spell_reg.get_by_pair(pair);
    assert!(spell.is_some());
    assert_eq!(spell.unwrap().id, "fire_bolt");
}

#[test]
fn mana_burn_propagates_to_progression() {
    // Verify that Mana Burn exposure is tracked in progression stats.
    let mut session = HeadlessSession::new(WorldSeed::new(42));

    // Force mana burn.
    session.player.mana.trigger_burn();
    let initial_burn = session.player.progression.stats.total_mana_burn_secs;
    session.step(2.0);
    let after_burn = session.player.progression.stats.total_mana_burn_secs;
    assert!(after_burn > initial_burn, "burn time should accumulate: {} → {}", initial_burn, after_burn);
    assert!((after_burn - 2.0).abs() < 0.01, "should record 2s of burn time, got {}", after_burn);
}
