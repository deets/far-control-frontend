use std::time::Instant;

#[cfg(feature = "novaview")]
use control_frontend::{
    rqprotocol::Node,
    telemetry::{setup_telemetry, Config},
};
use log::info;
use nanomsg::{Protocol, Socket};
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    node: Node,
    data: [u8; 32],
}

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
    use std::time::Duration;

    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    let mut telemetry = setup_telemetry(
        [
            Config {
                node: Node::RedQueen(b'B'),
                channel: 0,
            },
            Config {
                node: Node::RedQueen(b'T'),
                channel: 125,
            },
            Config {
                node: Node::Farduino(b'T'),
                channel: 32,
            },
            Config {
                node: Node::Farduino(b'B'),
                channel: 64,
            },
        ]
        .into_iter(),
    )?;

    let mut socket = Socket::new(Protocol::Pair)?;
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
                let _ = socket.nb_write(&j.as_bytes());
            }
            control_frontend::telemetry::TelemetryData::NoModule(node) => {
                println!("{:?} not connected", node);
                let kbps = count as f64 / 1000.0 / (Instant::now() - start).as_secs_f64();
                println!("{:.3}kb/s", kbps);
            }
        }) {}
        for node in [Node::RedQueen(b'B'), Node::RedQueen(b'T')] {
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
