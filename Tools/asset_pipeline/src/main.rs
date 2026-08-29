//! Arcane Asset Pipeline — CLI tool for cooking and validating source assets.
//!
//! Invocation examples:
//!   arcane_cook cook path/to/asset.gltf --out Assets/cooked/
//!   arcane_cook validate Assets/
//!   arcane_cook atlas Assets/textures/*.png --out Assets/cooked/atlas.bin

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("Arcane Asset Pipeline v{}", env!("CARGO_PKG_VERSION"));
    if args.len() < 2 {
        eprintln!("usage: arcane_cook <cook|validate|atlas> [args]");
        std::process::exit(2);
    }
    eprintln!("subcommand '{}' not yet implemented; foundation commit", args[1]);
    std::process::exit(0);
}
