use std::time::Duration;

use control_frontend::{
    rqparser::{verify_nmea_format, SentenceParser, MAX_BUFFER_SIZE},
    rqprotocol::Transaction,
};

#[cfg(feature = "e32")]
use embedded_hal::serial::{Read, Write};
#[cfg(feature = "e32")]
type E32Connection = control_frontend::ebyte::E32Connection;

use log::{error, info};
use nb::block;

const DEVICE: &str = "/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A100X7AI-if00-port0";

#[cfg(feature = "e32")]
fn main() -> anyhow::Result<()> {
    simple_logger::init_with_env().unwrap();
    info!("Opening E32 {}", DEVICE);
    let mut conn = E32Connection::raw_module(DEVICE)?;
    let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
    let mut sentence_parser = SentenceParser::new(&mut ringbuffer);
    loop {
        match block!(conn.read()) {
            Ok(b) => {
                let mut sentence: Option<Vec<u8>> = None;
                sentence_parser
                    .feed(&[b], |sentence_| sentence = Some(sentence_.to_vec()))
                    .expect("error parsing sentence");
                if let Some(sentence) = sentence {
                    info!("Got sencence {:?}", std::str::from_utf8(&sentence));
                    let sentence = verify_nmea_format(&sentence).unwrap();
                    let mut dest = [0; MAX_BUFFER_SIZE];
                    let input = &sentence[0..sentence.len()];
                    dest[0..sentence.len()].copy_from_slice(input);
                    let t = Transaction::from_sentence(input)?;
                    let response = t.acknowledge(&mut dest)?;
                    info!("Ack {:?}", std::str::from_utf8(&response).unwrap());
                    conn.write_buffer(response)?;
                    std::thread::sleep(Duration::from_millis(500))
                }
            }
            Err(err) => match err.kind() {
                std::io::ErrorKind::TimedOut => {}
                _ => {
                    error!("{:?}", err);
                }
            },
        }
    }
    Ok(())
}

#[cfg(not(feature = "e32"))]
fn main() -> anyhow::Result<()> {
    Ok(())
}
