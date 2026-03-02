use arb_risk::kill_switch::KillSwitch;

pub fn execute() -> anyhow::Result<()> {
    let mut ks = KillSwitch::new();
    if ks.is_active() {
        ks.deactivate();
        println!("Kill switch DEACTIVATED. Trading may resume.");
    } else {
        println!("Kill switch is not active.");
    }
    Ok(())
}
