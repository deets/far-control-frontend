use control_frontend::{
    rqprotocol::Node,
    telemetry::{setup_telemetry, Config},
};
use log::info;
use nanomsg::{Protocol, Socket};

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
    loop {
        telemetry.recv(|data| match data {
            control_frontend::telemetry::TelemetryData::Frame(node, data) => {
                println!("{:?}, {:?}", node, data);
                let _ = socket.nb_write(&data);
            }
            control_frontend::telemetry::TelemetryData::NoModule(node) => {
                println!("{:?} not connected", node);
            }
        });
    }
    //    Ok(())
}
