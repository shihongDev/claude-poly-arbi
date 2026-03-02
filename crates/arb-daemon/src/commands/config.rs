use arb_core::config::ArbConfig;

pub fn execute() -> anyhow::Result<()> {
    let path = ArbConfig::default_path();

    println!("=== Arb Configuration ===\n");
    println!("Config file: {}\n", path.display());

    if path.exists() {
        match ArbConfig::load_from(&path) {
            Ok(config) => {
                let toml = toml::to_string_pretty(&config)?;
                println!("{toml}");
                println!("\nConfig is valid.");
            }
            Err(e) => {
                println!("Config ERROR: {e}");
                println!("\nFalling back to defaults:");
                let config = ArbConfig::default();
                let toml = toml::to_string_pretty(&config)?;
                println!("{toml}");
            }
        }
    } else {
        println!("No config file found. Using defaults:\n");
        let config = ArbConfig::default();
        let toml = toml::to_string_pretty(&config)?;
        println!("{toml}");

        println!("\nTo create a config file:");
        println!("  mkdir -p {}", path.parent().unwrap().display());
        println!("  arb config > {}", path.display());
    }

    Ok(())
}
