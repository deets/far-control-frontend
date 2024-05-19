use std::path::PathBuf;

use anyhow::anyhow;
use embedded_hal::blocking::spi::Transfer;
use embedded_hal::digital::v2::OutputPin;
use embedded_nrf24l01::StandbyMode;
use embedded_nrf24l01::{Configuration, CrcMode, DataRate, NRF24L01};
use linux_embedded_hal::spidev::spidevioctl::spi_ioc_transfer;
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{SpiModeFlags, Spidev, SpidevOptions},
    CdevPin, CdevPinError,
};
use log::{debug, info};

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

type SpiError = embedded_nrf24l01::Error<std::io::Error>;
type NRFStandby = StandbyMode<NRF24L01<CdevPinError, CEPin, NullPin, SpiWrapper>>;

fn setup_nrf(ce_pin: CEPin, spi: SpiWrapper) -> core::result::Result<NRFStandby, SpiError> {
    let mut nrf24 = NRF24L01::new(ce_pin, NullPin {}, spi)?;
    nrf24.set_auto_retransmit(0, 0)?;
    nrf24.set_rf(&DataRate::R250Kbps, 3)?;
    nrf24.set_auto_ack(&[false; 6])?;
    nrf24.set_crc(CrcMode::TwoBytes)?;
    nrf24.set_tx_addr(&b"RQARQ"[..])?;
    nrf24.set_rx_addr(0, &b"RQARQ"[..])?;
    nrf24.set_pipes_rx_lengths(&[Some(32); 6])?;
    nrf24.set_frequency(0)?;
    Ok(nrf24)
}

fn main() -> anyhow::Result<()> {
    simple_logger::init_with_env().unwrap();
    info!("NRF TEST");
    let mut chip = Chip::new::<PathBuf>("/dev/gpiochip0".into())?;
    let ce_pin3 = chip
        .get_line(27)?
        .request(LineRequestFlags::OUTPUT, 0, "e32linux")?;
    let ce_pin3 = CEPin {
        pin: CdevPin::new(ce_pin3)?,
    };
    let ce_pin2 = chip
        .get_line(26)?
        .request(LineRequestFlags::OUTPUT, 0, "e32linux")?;
    let ce_pin2 = CEPin {
        pin: CdevPin::new(ce_pin2)?,
    };
    let spi10 = SpiWrapper(create_spi("/dev/spidev1.0")?);
    let spi11 = SpiWrapper(create_spi("/dev/spidev1.1")?);
    let nrf24 = match setup_nrf(ce_pin2, spi10) {
        Ok(nrf) => Ok(nrf),
        Err(_) => Err(anyhow!("Can't setup NRF24")),
    };

    Ok(())
}
