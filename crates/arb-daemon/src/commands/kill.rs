use arb_risk::kill_switch::KillSwitch;

pub fn execute() -> anyhow::Result<()> {
    let mut ks = KillSwitch::new();
    ks.activate("Manual kill via CLI");
    println!("Kill switch ACTIVATED. All trading halted.");
    println!("Run `arb resume` to re-enable trading.");
    Ok(())
}
