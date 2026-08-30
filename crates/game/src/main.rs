//! Mystical Arcana — main binary entry point.
//!
//! Supports the existing `--headless --smoke` gameplay loop AND the new
//! renderer-driven flags (`--observatory`, `--visualize`, `--scenario`,
//! `--output`, `--backend`, etc.).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("{} v{} — running on the {} engine",
        mystical_arcana_lib::GAME_NAME,
        mystical_arcana_lib::VERSION,
        mystical_arcana_lib::ENGINE_NAME);

    // The existing --headless --smoke path is preserved inside the unified CLI.
    match mystical_arcana_lib::cli::parse_cli(args) {
        Ok(opts) => {
            if let Err(e) = mystical_arcana_lib::cli::run(opts) {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprintln!("usage: mystical_arcana [--headless] [--smoke] [--visualize] [--observatory]");
            eprintln!("                     [--scenario NAME] [--frame N] [--frames N]");
            eprintln!("                     [--output PATH | --capture PATH] [--backend cpu|vulkan|auto]");
            eprintln!("                     [--port N] [--width N] [--height N] [--seed N] [--sim SECONDS]");
            eprintln!();
            eprintln!("scenarios:");
            for s in ["empty_scene", "basic_scene", "terrain_scene", "player_scene",
                      "mana_node_scene", "combat_scene", "building_scene", "corruption_scene"] {
                eprintln!("  {s}");
            }
            std::process::exit(1);
        }
    }
}
