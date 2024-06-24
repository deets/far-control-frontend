use serial_core::{BaudRate, CharSize, FlowControl, Parity, PortSettings, SerialPort, StopBits};
use std::io::Read;

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
    let comms_settings: PortSettings = PortSettings {
        baud_rate: BaudRate::BaudOther(460800),
        //baud_rate: BaudRate::BaudOther(230400),
        //baud_rate: BaudRate::Baud115200,
        char_size: CharSize::Bits8,
        parity: Parity::ParityNone,
        stop_bits: StopBits::Stop1,
        flow_control: FlowControl::FlowNone,
    };
    let mut port = ::serial::open("/dev/ttyAMA3")?;
    port.configure(&comms_settings)?;
    loop {
        let mut buf: [u8; 256] = [0; 256];
        match port.read(&mut buf) {
            Ok(num) => match std::str::from_utf8(&buf[0..num]) {
                Ok(s) => print!("{}", s),
                Err(_) => {}
            },
            Err(err) => {
                println!("{:?}", err);
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "novaview"))]
fn main() {}
