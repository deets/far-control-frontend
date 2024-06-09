use std::collections::HashMap;
use std::io::Empty;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use crossbeam_channel::{unbounded, Receiver, Sender};
use embedded_hal::blocking::spi::Transfer;
use embedded_hal::digital::v2::OutputPin;
use embedded_nrf24l01::{Configuration, CrcMode, DataRate, NRF24L01};
use embedded_nrf24l01::{RxMode, StandbyMode};
use linux_embedded_hal::spidev::spidevioctl::spi_ioc_transfer;
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{SpiModeFlags, Spidev, SpidevOptions},
    CdevPin, CdevPinError,
};
use log::{error, info, warn};

use crate::rqprotocol::Node;

type SpiError = embedded_nrf24l01::Error<std::io::Error>;
type NRFStandby = StandbyMode<NRF24L01<CdevPinError, CEPin, NullPin, SpiWrapper>>;
type NRFRx = RxMode<NRF24L01<CdevPinError, CEPin, NullPin, SpiWrapper>>;

const PIPE_ADDRESS: &[u8] = b"RQARQ";

struct NullPin {}
impl OutputPin for NullPin {
    type Error = CdevPinError;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct CEPin {
    pin: CdevPin,
}

impl OutputPin for CEPin {
    type Error = CdevPinError;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(0)?;
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(1)?;
        Ok(())
    }
}

struct SpiWrapper(Spidev);

impl Transfer<u8> for SpiWrapper {
    type Error = std::io::Error;
    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        let mut inbuffer = [0; 256];
        let mut t = spi_ioc_transfer::read_write(words, &mut inbuffer[0..words.len()]);
        self.0.transfer(&mut t)?;
        for i in 0..words.len() {
            words[i] = inbuffer[i];
        }
        Ok(words)
    }
}

fn create_spi(spi_dev_path: &str) -> std::io::Result<Spidev> {
    let mut spi = Spidev::open(spi_dev_path)?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(8_000_000)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)?;
    Ok(spi)
}

fn setup_nrf(ce_pin: CEPin, spi: SpiWrapper) -> core::result::Result<NRFStandby, SpiError> {
    let mut nrf24 = NRF24L01::new(ce_pin, NullPin {}, spi)?;
    nrf24.set_auto_retransmit(0, 0)?;
    nrf24.set_rf(&DataRate::R250Kbps, 3)?;
    nrf24.set_auto_ack(&[false; 6])?;
    nrf24.set_crc(CrcMode::TwoBytes)?;
    nrf24.set_tx_addr(&PIPE_ADDRESS[..])?;
    nrf24.set_rx_addr(0, &PIPE_ADDRESS[..])?;
    nrf24.set_pipes_rx_lengths(&[Some(32); 6])?;
    Ok(nrf24)
}

fn create_nrf_module(chip: &mut Chip, ce_pin: u32, device: &str) -> anyhow::Result<NRFStandby> {
    let ce_pin = chip
        .get_line(ce_pin)?
        .request(LineRequestFlags::OUTPUT, 0, "e32linux")?;
    let ce_pin = CEPin {
        pin: CdevPin::new(ce_pin)?,
    };
    let spi = SpiWrapper(create_spi(device)?);
    let nrf24 = match setup_nrf(ce_pin, spi) {
        Ok(nrf) => Ok(nrf),
        Err(_) => Err(anyhow!("Can't setup NRF24")),
    }?;
    Ok(nrf24)
}

enum NRFEntry {
    Working(NRFStandby),
    Unavailable,
}

fn enumerate_nrf_modules(chip: &mut Chip) -> impl Iterator<Item = NRFEntry> {
    let mut nrfs = vec![];
    for (ce_pin, device) in [
        (22, "/dev/spidev0.0"),
        (25, "/dev/spidev0.1"),
        (26, "/dev/spidev1.0"),
        (27, "/dev/spidev1.1"),
    ] {
        match create_nrf_module(chip, ce_pin, device) {
            Ok(nrf) => {
                info!("NRF on {}", device);
                nrfs.push(NRFEntry::Working(nrf));
            }
            Err(err) => {
                warn!("Can't setup NRF on {}, {:?}", device, err);
                nrfs.push(NRFEntry::Unavailable);
            }
        }
    }
    nrfs.into_iter()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub node: Node,
    pub channel: u8,
}

#[derive(Debug)]
pub enum TelemetryData {
    Frame(Node, Vec<u8>),
    NoModule(Node),
}

#[derive(Debug)]
enum NRFOrDummy {
    Working(NRFRx),
    Dummy(Instant),
}

impl NRFOrDummy {
    fn read(&mut self, res: &mut Vec<TelemetryData>, node: Node) {
        match self {
            NRFOrDummy::Working(nrf) => {
                if let Some(_) = nrf.can_read().unwrap() {
                    let payload = nrf.read().unwrap();
                    let data: &[u8] = &payload;
                    if data.len() > 0 {
                        let data = TelemetryData::Frame(node, data.into());
                        res.push(data);
                    }
                }
            }
            NRFOrDummy::Dummy(last_timestamp) => {
                let elapsed = Instant::now() - *last_timestamp;
                if elapsed.as_secs() > 5 {
                    *last_timestamp = Instant::now();
                    res.push(TelemetryData::NoModule(node));
                }
            }
        }
    }
}

#[derive(Debug)]
struct TelemetryConnection {
    nrf: NRFOrDummy,
    node: Node,
}

impl TelemetryConnection {
    fn new(config: Config, nrf: NRFEntry) -> Self {
        let nrf = match nrf {
            NRFEntry::Working(mut nrf) => match nrf.set_frequency(config.channel) {
                Ok(_) => match nrf.rx() {
                    Ok(rx_nrf) => NRFOrDummy::Working(rx_nrf),
                    Err(_) => {
                        warn!("Can't get module into RX mode");
                        NRFOrDummy::Dummy(Instant::now())
                    }
                },
                Err(_) => {
                    warn!("Can't set frequency for {:?}", config);
                    NRFOrDummy::Dummy(Instant::now())
                }
            },
            NRFEntry::Unavailable => NRFOrDummy::Dummy(Instant::now()),
        };
        Self {
            node: config.node,
            nrf,
        }
    }

    fn read(&mut self) -> Vec<TelemetryData> {
        let mut res = vec![];
        self.nrf.read(&mut res, self.node);
        res
    }
}

pub struct TelemetryEndpoint {
    worker: Option<JoinHandle<()>>,
    command_receiver: Receiver<TelemetryData>,
    running: Arc<Mutex<bool>>,
    start: Instant,
    last_comms: HashMap<Node, Instant>,
    registered_nodes: Vec<Node>,
}

impl TelemetryEndpoint {
    pub fn recv(&mut self, mut callback: impl FnMut(TelemetryData)) -> bool {
        let mut received_something = false;
        match self.command_receiver.try_recv() {
            Ok(data) => {
                if let TelemetryData::Frame(node, _) = data {
                    self.last_comms.insert(node, Instant::now());
                }
                received_something = true;
                callback(data);
            }
            Err(err) => match err {
                crossbeam_channel::TryRecvError::Empty => {}
                crossbeam_channel::TryRecvError::Disconnected => {
                    error!("Can't read from cb channel: {:?}", err);
                    panic!();
                }
            },
        }
        received_something
    }

    pub fn heard_from_since(&self, node: &Node) -> Duration {
        Instant::now()
            - if self.last_comms.contains_key(node) {
                self.last_comms[node]
            } else {
                self.start
            }
    }

    pub fn registered_nodes(&self) -> &Vec<Node> {
        &self.registered_nodes
    }

    fn quit(&mut self) {
        {
            let mut running = self.running.lock().unwrap();
            *running = false;
        }
        // See https://stackoverflow.com/questions/57670145/how-to-store-joinhandle-of-a-thread-to-close-it-later
        self.worker.take().map(JoinHandle::join);
    }
}

impl Drop for TelemetryEndpoint {
    fn drop(&mut self) {
        self.quit();
    }
}
fn work(
    sender: Sender<TelemetryData>,
    mut connections: Vec<TelemetryConnection>,
    running: Arc<Mutex<bool>>,
) {
    loop {
        let mut sent = false;
        for conn in connections.iter_mut() {
            for data in conn.read() {
                sent = true;
                sender.send(data).expect("crossbeam not working");
            }
        }
        if !sent {
            thread::sleep(Duration::from_millis(10));
        }
        {
            let r = running.lock().unwrap();
            if !*r {
                break;
            }
        }
    }
}

pub fn setup_telemetry(configs: impl Iterator<Item = Config>) -> anyhow::Result<TelemetryEndpoint> {
    let mut chip = Chip::new::<PathBuf>("/dev/gpiochip0".into())?;
    let mut registered_nodes = vec![];
    let nrf_modules = enumerate_nrf_modules(&mut chip).collect::<Vec<NRFEntry>>();
    let configs = configs.collect::<Vec<Config>>();
    if nrf_modules.len() < configs.len() {
        warn!(
            "Not enough modules for {} configurations, only configuring the first {}",
            configs.len(),
            nrf_modules.len()
        );
    }
    let mut connections = vec![];
    for (config, nrf) in configs.into_iter().zip(nrf_modules.into_iter()) {
        registered_nodes.push(config.node.clone());
        let conn = TelemetryConnection::new(config, nrf);
        connections.push(conn);
    }
    let running = Arc::new(Mutex::new(true));
    let worker_running = running.clone();
    let (command_sender, command_receiver) = unbounded::<TelemetryData>();
    let handle = thread::spawn(move || {
        work(command_sender, connections, worker_running);
    });

    Ok(TelemetryEndpoint {
        command_receiver,
        worker: Some(handle),
        running,
        start: Instant::now(),
        last_comms: HashMap::new(),
        registered_nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MessageParser {
        buffer: Vec<u8>,
    }

    impl Default for MessageParser {
        fn default() -> Self {
            Self {
                buffer: Default::default(),
            }
        }
    }

    impl MessageParser {
        fn feed(&mut self, data: &[u8], callback: impl FnMut(&[u8])) {}
    }

    #[test]
    fn test_message_parser() {
        // let mut parser = MessageParser::default();
        // let mut result = vec![];
        // let a = b"$1,0BEBC200,0000010495916A20,000";
        // let b = b"70EE1\x13\x10\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        // assert_eq!(a.len(), 32);
        // assert_eq!(b.len(), 32);
        // parser.feed(a, |message| {
        //     let message: Vec<u8> = message.into();
        //     result.push(message);
        // });
        // assert_eq!(result.len(), 1);
    }
}
