//! Compile-time [`Id64`](arcane_core::id::Id64) constants for engine
//! and game subsystems. Centralized so that the same string hashes to the
//! same value everywhere.

// Engine subsystem names (used for logging, profiling, ECS component IDs).
arcane_core::ident!(ENGINE, "engine");
arcane_core::ident!(RENDER, "render");
arcane_core::ident!(WORLD, "world");
arcane_core::ident!(PHYSICS, "physics");
arcane_core::ident!(AUDIO, "audio");
arcane_core::ident!(INPUT, "input");
arcane_core::ident!(VFX, "vfx");
arcane_core::ident!(UI, "ui");
arcane_core::ident!(STREAMING, "streaming");
arcane_core::ident!(SAVE, "save");
arcane_core::ident!(PROFILING, "profiling");

// === Magical categories (game-side) ===

arcane_core::ident!(MANA, "mana");
arcane_core::ident!(RUNE, "rune");
arcane_core::ident!(SPELL, "spell");
arcane_core::ident!(SCHEMATIC, "schematic");
arcane_core::ident!(INVENTORY, "inventory");
arcane_core::ident!(COMBAT, "combat");
arcane_core::ident!(ENEMY, "enemy");
arcane_core::ident!(CORRUPTION, "corruption");
arcane_core::ident!(BUILDING, "building");
arcane_core::ident!(SANCTUARY, "sanctuary");
arcane_core::ident!(RESEARCH, "research");
arcane_core::ident!(PROGRESSION, "progression");

// === Rune categories (data-driven rune defs use these) ===

arcane_core::ident!(RUNE_MOVEMENT, "rune/movement");
arcane_core::ident!(RUNE_TRANSFORMATION, "rune/transformation");
arcane_core::ident!(RUNE_PROTECTION, "rune/protection");
arcane_core::ident!(RUNE_DESTRUCTION, "rune/destruction");
arcane_core::ident!(RUNE_MANIPULATION, "rune/manipulation");

// === Resource categories ===

arcane_core::ident!(RES_PLANT, "resource/plant");
arcane_core::ident!(RES_STONE, "resource/stone");
arcane_core::ident!(RES_CRYSTAL, "resource/crystal");
arcane_core::ident!(RES_MANA, "resource/mana");

#[cfg(test)]
mod tests {
    use arcane_core::Id64;

    #[test]
    fn engine_id_is_nonzero() {
        // NULL is, by definition, null.
        assert!(Id64::NULL.is_null());
        // Re-export of ENGINE should produce a non-null ID.
        let engine_id = crate::ids::ENGINE;
        assert!(!engine_id.is_null(), "ENGINE id should not be null");
    }
}
