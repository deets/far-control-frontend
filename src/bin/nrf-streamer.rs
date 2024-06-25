use std::time::Instant;

#[cfg(feature = "novaview")]
use control_frontend::{rqprotocol::Node, telemetry::create};
use log::info;

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
    use std::time::Duration;

    use control_frontend::telemetry::ZMQPublisher;

    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    let mut publisher = ZMQPublisher::new("tcp://0.0.0.0:2424")?;
    let telemetry = create();
    loop {
        publisher.publish_telemetry_data(&telemetry.borrow_mut().drive());
        for node in telemetry.borrow().registered_nodes() {
            println!(
                "last heard of {:?}: {:?}",
                &node,
                telemetry.borrow().heard_from_since(&node)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    //    Ok(())
}

#[cfg(not(feature = "novaview"))]
fn main() {}
