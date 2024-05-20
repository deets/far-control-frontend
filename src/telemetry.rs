use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

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
use log::{error, warn};

use crate::rqprotocol::Node;

type SpiError = embedded_nrf24l01::Error<std::io::Error>;
type NRFStandby = StandbyMode<NRF24L01<CdevPinError, CEPin, NullPin, SpiWrapper>>;
type NRFRx = RxMode<NRF24L01<CdevPinError, CEPin, NullPin, SpiWrapper>>;

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
    nrf24.set_tx_addr(&b"RQARQ"[..])?;
    nrf24.set_rx_addr(0, &b"RQARQ"[..])?;
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

fn enumerate_nrf_modules(chip: &mut Chip) -> impl Iterator<Item = NRFStandby> {
    let mut nrfs = vec![];
    for (ce_pin, device) in [(22, "/dev/spidev0.0")] {
        match create_nrf_module(chip, ce_pin, device) {
            Ok(nrf) => nrfs.push(nrf),
            Err(err) => {
                error!("Can't setup NRF on {}, {:?}", device, err);
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

struct TelemetryConnection {
    nrf: NRFRx,
    node: Node,
}

#[derive(Debug)]
pub struct TelemetryData {
    pub source: Node,
    pub data: Vec<u8>,
}

pub struct TelemetryEndpoint {
    worker: Option<JoinHandle<()>>,
    command_receiver: Receiver<TelemetryData>,
    running: Arc<Mutex<bool>>,
}

impl TelemetryEndpoint {
    pub fn recv(&mut self, callback: impl FnOnce(TelemetryData)) {
        match self.command_receiver.recv() {
            Ok(data) => {
                callback(data);
            }
            Err(err) => {
                error!("Can't read from cb channel: {:?}", err);
            }
        }
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
        for conn in connections.iter_mut() {
            while let Some(_) = conn.nrf.can_read().unwrap() {
                let payload = conn.nrf.read().unwrap();
                let data: &[u8] = &payload;
                let data = TelemetryData {
                    source: conn.node,
                    data: data.into(),
                };
                sender.send(data).expect("crossbeam not working");
            }
        }
        {
            let mut r = running.lock().unwrap();
            if !*r {
                break;
            }
        }
    }
}

pub fn setup_telemetry(configs: impl Iterator<Item = Config>) -> anyhow::Result<TelemetryEndpoint> {
    let mut chip = Chip::new::<PathBuf>("/dev/gpiochip0".into())?;
    let nrf_modules = enumerate_nrf_modules(&mut chip).collect::<Vec<NRFStandby>>();
    let configs = configs.collect::<Vec<Config>>();
    if nrf_modules.len() < configs.len() {
        warn!(
            "Not enough modules for configuration, only configuring the first {}",
            configs.len()
        );
    }
    let mut connections = vec![];
    for (config, mut nrf) in configs.into_iter().zip(nrf_modules.into_iter()) {
        match nrf.set_frequency(config.channel) {
            Ok(_) => {
                if let Some(conn) = match nrf.rx() {
                    Ok(rx_nrf) => Some(TelemetryConnection {
                        nrf: rx_nrf,
                        node: config.node,
                    }),
                    Err(_) => {
                        warn!("Can't get module into RX mode");
                        None
                    }
                } {
                    connections.push(conn);
                };
            }
            Err(_) => {
                warn!("Can't set frequency for {:?}", config);
            }
        }
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
    })
}
