use control_frontend::telemetry::setup_telemetry;
use log::info;

fn main() -> anyhow::Result<()> {
    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    setup_telemetry()?;
    Ok(())
}
