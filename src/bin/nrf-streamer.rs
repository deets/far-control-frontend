use std::time::Instant;

#[cfg(feature = "novaview")]
use control_frontend::{
    rqprotocol::Node,
    telemetry::{setup_telemetry, Config},
};
use log::info;
use serde::Serialize;
#[cfg(feature = "novaview")]
use zmq::{Context, Socket};

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
    use std::time::Duration;

    use control_frontend::telemetry::{Message, DEFAULT_CONFIGURATION};

    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    let mut telemetry = setup_telemetry(DEFAULT_CONFIGURATION.into_iter())?;

    let context = Context::new();
    let mut socket = context.socket(zmq::PUB)?;
    socket.bind("tcp://0.0.0.0:2424")?;
    let mut count = 0;
    let start = Instant::now();
    loop {
        while telemetry.recv(|data| match data {
            control_frontend::telemetry::TelemetryData::Frame(node, data) => {
                count += data.len() * 8;
                let message = Message {
                    node,
                    data: data.try_into().unwrap(),
                };
                let j = serde_json::to_string(&message).unwrap();
                let _ = socket.send(&j.as_bytes(), 0);
            }
            control_frontend::telemetry::TelemetryData::NoModule(node) => {
                println!("{:?} not connected", node);
                let kbps = count as f64 / 1000.0 / (Instant::now() - start).as_secs_f64();
                println!("{:.3}kb/s", kbps);
            }
        }) {}
        for node in telemetry.registered_nodes() {
            println!(
                "last heard of {:?}: {:?}",
                &node,
                telemetry.heard_from_since(&node)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    //    Ok(())
}

#[cfg(not(feature = "novaview"))]
fn main() {}
