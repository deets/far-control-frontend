use std::{cell::RefCell, rc::Rc, thread, time::Duration};

use embedded_hal::{
    blocking::delay::DelayMs,
    digital::v2::{InputPin, OutputPin},
    serial::{Read, Write},
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    CdevPin,
};
use serial_core::SerialPort;

type PortType = Rc<RefCell<dyn SerialPort>>;

pub struct CtsAux {
    pin: CdevPin,
}
pub struct M0Dtr {
    pin: CdevPin,
}
pub struct M1Rts {
    pin: CdevPin,
}
pub struct StandardDelay {}
pub struct Serial {
    port: PortType,
}

impl InputPin for CtsAux {
    type Error = serial::Error;

    fn is_high(&self) -> Result<bool, Self::Error> {
        match self.pin.get_value().unwrap() {
            0 => Ok(false),
            1 => Ok(true),
            _ => unreachable!(),
        }
    }

    fn is_low(&self) -> Result<bool, Self::Error> {
        Ok(!self.is_high().unwrap())
    }
}

impl OutputPin for M0Dtr {
    type Error = serial::Error;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(0).unwrap();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(1).unwrap();
        Ok(())
    }
}

impl OutputPin for M1Rts {
    type Error = serial::Error;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(0).unwrap();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.pin.set_value(1).unwrap();
        Ok(())
    }
}

impl Serial {
    pub fn new(port: PortType) -> Self {
        Serial { port }
    }
}
impl Read<u8> for Serial {
    type Error = std::io::Error;

    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        let mut result = [0];
        self.port.borrow_mut().read(&mut result)?;
        Ok(result[0])
    }
}

impl Write<u8> for Serial {
    type Error = std::io::Error;

    fn write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        let buf = [word];
        self.port.borrow_mut().write_all(&buf)?;
        Ok(())
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        self.port.borrow_mut().flush()?;
        Ok(())
    }
}

impl DelayMs<u32> for StandardDelay {
    fn delay_ms(&mut self, ms: u32) {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

impl CtsAux {
    pub fn new(chip: &mut Chip) -> anyhow::Result<Self> {
        let aux = chip
            .get_line(16)?
            .request(LineRequestFlags::INPUT, 0, "e32linux")?;
        let pin = CdevPin::new(aux)?;
        Ok(Self { pin })
    }
}

impl M0Dtr {
    pub fn new(chip: &mut Chip) -> anyhow::Result<Self> {
        let pin = chip
            .get_line(23)?
            .request(LineRequestFlags::OUTPUT, 0, "e32linux")?;
        let pin = CdevPin::new(pin)?;
        Ok(Self { pin })
    }
}

impl M1Rts {
    pub fn new(chip: &mut Chip) -> anyhow::Result<Self> {
        let pin = chip
            .get_line(24)?
            .request(LineRequestFlags::OUTPUT, 0, "e32linux")?;
        let pin = CdevPin::new(pin)?;
        Ok(Self { pin })
    }
}
