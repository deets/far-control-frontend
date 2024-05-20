use control_frontend::{
    rqprotocol::Node,
    telemetry::{setup_telemetry, Config},
};
use log::info;

fn main() -> anyhow::Result<()> {
    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    let mut telemetry = setup_telemetry(
        [Config {
            node: Node::RedQueen(b'A'),
            channel: 0,
        }]
        .into_iter(),
    )?;
    for _ in 0..100 {
        telemetry.recv(|data| {
            println!("{:?}", data);
        });
    }

    Ok(())
}
