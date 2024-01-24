use crossbeam_channel::{unbounded, Receiver, Sender};
use ebyte_e32::{mode::Normal, Ebyte, Parameters};
use ebyte_e32_ftdi::{CtsAux, M0Dtr, M1Rts, Serial, StandardDelay};
use serial_core::{BaudRate, CharSize, FlowControl, Parity, PortSettings, SerialPort, StopBits};
use std::{
    cell::RefCell,
    rc::Rc,
    thread::{self, JoinHandle},
    time::Duration,
};

type E32Module = Ebyte<Serial, CtsAux, M0Dtr, M1Rts, StandardDelay, Normal>;

fn create(port: &str) -> anyhow::Result<E32Module> {
    let baud_rate = BaudRate::Baud9600;
    let stop_bits = StopBits::Stop1;

    let settings: PortSettings = PortSettings {
        baud_rate,
        char_size: CharSize::Bits8,
        parity: Parity::ParityNone,
        stop_bits,
        flow_control: FlowControl::FlowNone,
    };

    let mut port = ::serial::open(port)?;

    port.set_timeout(Duration::from_secs(200))?;
    port.configure(&settings)?;

    let port = Rc::new(RefCell::new(port));
    let serial = Serial::new(port.clone());

    let aux = CtsAux::new(port.clone());

    let m0 = M0Dtr::new(port.clone());
    let m1 = M1Rts::new(port.clone());
    let delay = StandardDelay {};
    Ok(Ebyte::new(serial, aux, m0, m1, delay)?)
}

pub struct E32Connection {
    worker: JoinHandle<()>,
    command_sender: Sender<Commands>,
}

enum Commands {
    Open(String),
    Send(Vec<u8>),
}

fn work(receiver: Receiver<Commands>) {
    let mut module = None;
    loop {
        match receiver.recv() {
            Ok(m) => match m {
                Commands::Open(port) => {
                    module = Some(create(&port).expect("Can't create port"));
                }
                Commands::Send(data) => match &mut module {
                    Some(module) => {
                        module.write_buffer(&data).expect("can't send data");
                    }
                    None => {
                        println!("No open E32 connection");
                    }
                },
            },
            Err(_) => {
                panic!("Crossbeam is angry");
            }
        }
    }
}

impl E32Connection {
    pub fn new(port: &str) -> anyhow::Result<E32Connection> {
        let port = port.to_string();
        let (command_sender, command_receiver) = unbounded::<Commands>();
        let handle = thread::spawn(move || {
            work(command_receiver);
        });
        command_sender.send(Commands::Open(port))?;
        Ok(E32Connection {
            worker: handle,
            command_sender,
        })
    }
}

fn default_parameters() -> Parameters {
    Parameters {
        address: 1234,
        channel: 0x17,
        uart_parity: ebyte_e32::parameters::Parity::None,
        uart_rate: ebyte_e32::parameters::BaudRate::Bps9600,
        air_rate: ebyte_e32::parameters::AirBaudRate::Bps2400,
        transmission_mode: ebyte_e32::parameters::TransmissionMode::Fixed,
        io_drive_mode: ebyte_e32::parameters::IoDriveMode::PushPull,
        wakeup_time: ebyte_e32::parameters::WakeupTime::Ms250,
        fec: ebyte_e32::parameters::ForwardErrorCorrectionMode::On,
        transmission_power: ebyte_e32::parameters::TransmissionPower::Dbm21,
    }
}
