use control_frontend::{ebyte::E32Connection, rqparser::SentenceParser};
use embedded_hal::serial::{Read, Write};
use nb::block;

fn main() -> anyhow::Result<()> {
    let mut conn = E32Connection::raw_module(
        "/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A100X7AI-if00-port0",
    )?;
    let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
    let mut sentence_parser = SentenceParser::new(&mut ringbuffer);
    loop {
        let b = block!(conn.read()).expect("Failed to read");
        sentence_parser
            .feed(&[b], |sentence| {
                let sentence = std::str::from_utf8(sentence).expect("no utf8");
                println!("{}", sentence);
            })
            .expect("error parsing sentence");
    }
    Ok(())
}
