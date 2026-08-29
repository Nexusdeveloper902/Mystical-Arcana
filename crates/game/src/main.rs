//! Mystical Arcana — main binary entry point.
//!
//! When invoked with `--headless --smoke`, runs the headless gameplay harness
//! and exits. Otherwise (the default) it would launch the Vulkan renderer +
//! window; in the current foundation commit, the binary simply reports that
//! the GPU surface is not yet implemented.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let smoke = args.iter().any(|a| a == "--smoke");

    println!("{} v{} — running on the {} engine", mystical_arcana_lib::GAME_NAME, mystical_arcana_lib::VERSION, mystical_arcana_lib::ENGINE_NAME);

    if headless && smoke {
        match mystical_arcana_lib::headless::run_until_complete(std::time::Duration::from_secs(2)) {
            Ok(elapsed) => {
                println!("[smoke] headless loop completed in {:.3}s", elapsed.as_secs_f32());
                println!("[smoke] OK");
                return;
            }
            Err(e) => {
                eprintln!("[smoke] FAILED: {:?}", e);
                std::process::exit(1);
            }
        }
    }

    if headless {
        println!("Headless mode active but no smoke target requested. Exiting cleanly.");
        return;
    }

    // The Vulkan surface is not yet implemented in this foundation commit.
    // Subsequent commits on `feature/arcane-render` will replace this branch.
    println!("Vulkan surface not yet available. Use --headless --smoke for the smoke test.");
}
