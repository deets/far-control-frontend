use anyhow::anyhow;
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

pub type E32Module = Ebyte<Serial, CtsAux, M0Dtr, M1Rts, StandardDelay, Normal>;

pub struct E32Connection {
    worker: JoinHandle<()>,
    command_sender: Sender<Commands>,
    response_receiver: Receiver<Answers>,
    busy: bool,
}

enum Commands {
    Open(String),
    Send(Vec<u8>),
}

enum Answers {
    Sent,
}

fn work(receiver: Receiver<Commands>, sender: Sender<Answers>) {
    let mut module = None;
    loop {
        match receiver.recv() {
            Ok(m) => match m {
                Commands::Open(port) => {
                    module = Some(create(&port, default_parameters()).expect("Can't create port"));
                }
                Commands::Send(data) => match &mut module {
                    Some(module) => {
                        module.write_buffer(&data).expect("can't send data");
                        std::thread::sleep(Duration::from_millis(1000));
                        sender.send(Answers::Sent).expect("can't ack data");
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
        let (response_sender, response_receiver) = unbounded::<Answers>();
        let handle = thread::spawn(move || {
            work(command_receiver, response_sender);
        });
        command_sender.send(Commands::Open(port))?;
        Ok(E32Connection {
            worker: handle,
            command_sender,
            response_receiver,
            busy: false,
        })
    }

    pub fn busy(&mut self) -> bool {
        match self.response_receiver.try_recv() {
            Ok(_) => {
                println!("unbusied");
                self.busy = false;
            }
            Err(_) => {}
        }
        self.busy
    }

    pub fn raw_module(port: &str) -> anyhow::Result<E32Module> {
        Ok(create(&port, default_parameters())?)
    }
}

impl std::io::Write for E32Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.command_sender
            .send(Commands::Send(buf.into()))
            .expect("crossbeam always works");
        self.busy = true;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn default_parameters() -> Parameters {
    Parameters {
        address: 1234,
        channel: 0x17,
        uart_parity: ebyte_e32::parameters::Parity::None,
        uart_rate: ebyte_e32::parameters::BaudRate::Bps9600,
        air_rate: ebyte_e32::parameters::AirBaudRate::Bps2400,
        transmission_mode: ebyte_e32::parameters::TransmissionMode::Transparent,
        io_drive_mode: ebyte_e32::parameters::IoDriveMode::PushPull,
        wakeup_time: ebyte_e32::parameters::WakeupTime::Ms250,
        fec: ebyte_e32::parameters::ForwardErrorCorrectionMode::On,
        transmission_power: ebyte_e32::parameters::TransmissionPower::Dbm21,
    }
}

fn create(port: &str, parameters: Parameters) -> anyhow::Result<E32Module> {
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
    let mut module = Ebyte::new(serial, aux, m0, m1, delay)?;
    configure(&mut module, &parameters)?;
    Ok(module)
}

fn configure(module: &mut E32Module, parameters: &Parameters) -> anyhow::Result<()> {
    // Cargo-cultish, but it appears I need to read once first.
    let _ = module.parameters()?;
    for i in 0..10 {
        module.set_parameters(parameters, ebyte_e32::parameters::Persistence::Permanent)?;
        let active = module.parameters()?;
        if active == *parameters {
            return Ok(());
        }
        println!("Parameters not set successfully, retrying {}", i);
        println!("active: {:?}, wanted: {:?}", active, *parameters);
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(anyhow!("Can't configure module!"))
}
