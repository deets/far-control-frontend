use std::time::Instant;

#[cfg(feature = "novaview")]
use control_frontend::{
    rqprotocol::Node,
    telemetry::{setup_telemetry, Config},
};
use log::info;
use nanomsg::{Protocol, Socket};

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
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
                channel: 12,
            },
            Config {
                node: Node::Farduino(b'T'),
                channel: 7,
            },
            Config {
                node: Node::Farduino(b'B'),
                channel: 100,
            },
        ]
        .into_iter(),
    )?;

    let mut socket = Socket::new(Protocol::Pair)?;
    socket.bind("tcp://0.0.0.0:2424")?;
    let mut count = 0;
    let start = Instant::now();
    loop {
        telemetry.recv(|data| match data {
            control_frontend::telemetry::TelemetryData::Frame(node, data) => {
                let _ = socket.nb_write(&data);
                count += data.len() * 8;
            }
            control_frontend::telemetry::TelemetryData::NoModule(node) => {
                println!("{:?} not connected", node);
                let kbps = count as f64 / 1000.0 / (Instant::now() - start).as_secs_f64();
                println!("{:.3}kb/s", kbps);
            }
        });
    }
    //    Ok(())
}

#[cfg(not(feature = "novaview"))]
fn main() {}
