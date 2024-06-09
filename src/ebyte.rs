use anyhow::anyhow;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use ebyte_e32::{mode::Normal, Ebyte, Parameters};

#[cfg(not(feature = "novaview"))]
use ebyte_e32_ftdi::{CtsAux, M0Dtr, M1Rts, Serial, StandardDelay};

use embedded_hal::serial::Read;
use log::{debug, error, info, warn};
use nb::block;

use serial_core::{BaudRate, CharSize, FlowControl, Parity, PortSettings, SerialPort, StopBits};
use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "novaview")]
use crate::e32linux::CtsAux;

use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    connection::{Answers, Connection},
    recorder::Recorder,
    rqparser::{SentenceParser, MAX_BUFFER_SIZE},
    rqprotocol::{Command, Node, Response, Transaction},
};

#[cfg(feature = "novaview")]
use crate::e32linux::{M0Dtr, M1Rts, Serial, StandardDelay};

const ANSWER_TIMEOUT: Duration = Duration::from_millis(100);

pub type E32Module = Ebyte<Serial, CtsAux, M0Dtr, M1Rts, StandardDelay, Normal>;

#[derive(Debug, PartialEq)]
enum Commands {
    Open(String),
    Send(Vec<u8>),
    Drain,
    Quit,
    Reset,
    Resume,
}

struct E32Worker<Id> {
    command_receiver: Receiver<Commands>,
    response_sender: Sender<Answers>,
    command_id_generator: Id,
    me: Node,
    target_red_queen: Node,
    recorder: Recorder,
}

pub struct E32Connection {
    worker: Option<JoinHandle<()>>,
    command_sender: Sender<Commands>,
    response_receiver: Receiver<Answers>,
    busy: bool,
}

impl E32Connection {
    pub fn new<Id: Iterator<Item = usize> + Send + Sync + 'static>(
        command_id_generator: Id,
        me: Node,
        target_red_queen: Node,
        recorder: Recorder,
    ) -> anyhow::Result<E32Connection> {
        let (command_sender, command_receiver) = unbounded::<Commands>();
        let (response_sender, response_receiver) = unbounded::<Answers>();
        let handle = thread::spawn(move || {
            let mut worker = E32Worker {
                command_receiver,
                response_sender,
                command_id_generator,
                me,
                target_red_queen,
                recorder,
            };
            worker.work();
        });
        Ok(E32Connection {
            worker: Some(handle),
            command_sender,
            response_receiver,
            busy: false,
        })
    }

    pub fn raw_module(port: &str) -> anyhow::Result<E32Module> {
        Ok(create(&port, default_parameters())?)
    }

    fn quit(&mut self) {
        self.command_sender.send(Commands::Quit).expect("crossbeam");
        // See https://stackoverflow.com/questions/57670145/how-to-store-joinhandle-of-a-thread-to-close-it-later
        self.worker.take().map(JoinHandle::join);
    }
}

impl Connection for E32Connection {
    fn recv(&mut self, callback: impl FnOnce(Answers)) {
        match self.response_receiver.try_recv() {
            Ok(answer) => {
                self.busy = false;
                callback(answer);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                panic!("Crossbeam channel to ebyte module disconnected!");
            }
        }
    }

    fn drain(&mut self) {
        self.command_sender.send(Commands::Drain).unwrap();
    }

    fn open(&mut self, port: &str) {
        self.command_sender
            .send(Commands::Open(port.into()))
            .unwrap();
    }

    fn reset(&mut self) {
        self.command_sender.send(Commands::Reset).unwrap();
    }

    fn resume(&mut self) {
        self.command_sender.send(Commands::Resume).unwrap();
    }
}

impl Drop for E32Connection {
    fn drop(&mut self) {
        info!("dropping E32Connection");
        self.quit();
    }
}

impl<Id> E32Worker<Id>
where
    Id: Iterator<Item = usize>,
{
    fn work(&mut self) {
        let mut module = None;
        let mut fetch_observables = false;
        loop {
            match self
                .command_receiver
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(m) => match m {
                    Commands::Reset => fetch_observables = false,
                    Commands::Resume => fetch_observables = true,
                    Commands::Quit => {
                        break;
                    }
                    Commands::Open(port) => match create(&port, default_parameters()) {
                        Ok(m) => {
                            module = Some(m);
                            self.response_sender.send(Answers::ConnectionOpen).unwrap();
                        }
                        Err(e) => {
                            error!("Can't open port {}, reason: {}", port, e);
                        }
                    },
                    Commands::Send(data) => match &mut module {
                        Some(module) => {
                            debug!("sending {}", std::str::from_utf8(&data).unwrap());
                            match module.write_buffer(&data) {
                                Ok(_) => {
                                    if Self::receive_sentence_or_timeout(
                                        module,
                                        |sentence| {
                                            self.response_sender
                                                .send(Answers::Received(sentence.clone()))
                                                .expect("can't ack data");
                                        },
                                        &mut self.recorder,
                                    ) {
                                        self.send_timeout();
                                    }
                                }
                                Err(err) => {
                                    error!("Sending data to module failed {:?}, sending Answers::ConnectionError", err);
                                    self.response_sender
                                        .send(Answers::ConnectionError)
                                        .expect("cc works!");
                                }
                            }
                        }
                        None => {
                            error!("No open E32 connection");
                            // To prevent spinning and log spam, wait a bit
                            std::thread::sleep(Duration::from_millis(500));
                            self.response_sender
                                .send(Answers::ConnectionError)
                                .expect("cc works");
                        }
                    },
                    Commands::Drain => {
                        if let Some(module) = &mut module {
                            self.drain(module);
                        }
                    }
                },
                Err(RecvTimeoutError::Timeout) => {
                    if fetch_observables {
                        if let Some(module) = &mut module {
                            self.fetch_observables(module);
                        }
                    }
                }
                Err(_) => {
                    panic!("Crossbeam is angry");
                }
            }
        }
    }

    // We just eat incoming bytes for 5 secs
    // before we go back to resetting.
    fn drain(&mut self, module: &mut E32Module) {
        warn!("Draining");
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            let _ = block!(module.read()).map(|c| self.recorder.store(c));
        }
        warn!("Drained");
        self.response_sender.send(Answers::Drained).unwrap();
    }

    fn fetch_observables(&mut self, module: &mut E32Module) {
        let id = self.command_id_generator.next().unwrap();
        let obg = if id % 5 == 0 { 2 } else { 1 };
        let mut t = Transaction::new(
            self.me,
            self.target_red_queen,
            id,
            Command::ObservableGroup(obg),
        );
        debug!("Send obg{} {}", obg, id);
        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        module.write_buffer(result).expect("can't send data");
        // First come the observables, so we relay them
        if Self::receive_sentence_or_timeout(
            module,
            |sentence| match t.process_response(sentence) {
                Ok(response) => {
                    if let Response::ObservableGroup(observables) = response {
                        self.response_sender
                            .send(Answers::Observables(observables))
                            .unwrap();
                    }
                }
                Err(_) => {
                    self.response_sender.send(Answers::ConnectionError).unwrap();
                    return;
                }
            },
            &mut self.recorder,
        ) {
            debug!("timeout getting OBG{} data", obg);
            self.send_timeout();
        } else {
            // now the ack is supposed to happen
            if Self::receive_sentence_or_timeout(
                module,
                |sentence| {
                    let _ = t.process_response(sentence);
                },
                &mut self.recorder,
            ) {
                debug!("timeout getting OBG{} ack", obg);
                self.send_timeout();
            }
        }
        debug!("finished obg{} keepalive", obg);
    }

    fn send_timeout(&mut self) {
        self.response_sender
            .send(Answers::Timeout)
            .expect("can't ack data");
    }

    fn receive_sentence_or_timeout(
        module: &mut E32Module,
        callback: impl FnOnce(&Vec<u8>),
        recorder: &mut Recorder,
    ) -> bool {
        let mut count = 0;
        let mut sentence_parser = SentenceParser::new();
        loop {
            match block!(module.read()) {
                Ok(b) => {
                    recorder.store(b);
                    let mut sentence: Option<Vec<u8>> = None;
                    sentence_parser
                        .feed(&[b], |sentence_| sentence = Some(sentence_.to_vec()))
                        .expect("error parsing sentence");
                    if let Some(sentence) = sentence {
                        debug!("got sentence: {}", std::str::from_utf8(&sentence).unwrap());
                        callback(&sentence);
                        return false;
                    }
                }
                Err(err) => match err.kind() {
                    std::io::ErrorKind::TimedOut => {
                        count += 1;
                        if count > 50 {
                            break;
                        }
                    }
                    _ => {
                        error!("Unhandled error: {:?}", err);
                        break;
                    }
                },
            }
        }
        true
    }
}

impl std::io::Write for E32Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        debug!("write: {}", std::str::from_utf8(buf).unwrap());
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
        address: 0x524F,
        channel: 0x17,
        uart_parity: ebyte_e32::parameters::Parity::None,
        uart_rate: ebyte_e32::parameters::BaudRate::Bps9600,
        //air_rate: ebyte_e32::parameters::AirBaudRate::Bps19200,
        air_rate: ebyte_e32::parameters::AirBaudRate::Bps9600,
        transmission_mode: ebyte_e32::parameters::TransmissionMode::Transparent,
        io_drive_mode: ebyte_e32::parameters::IoDriveMode::PushPull,
        wakeup_time: ebyte_e32::parameters::WakeupTime::Ms250,
        fec: ebyte_e32::parameters::ForwardErrorCorrectionMode::On,
        transmission_power: ebyte_e32::parameters::TransmissionPower::Dbm21,
    }
}

pub fn modem_baud_rate() -> BaudRate {
    #[cfg(not(target_os = "windows"))]
    return BaudRate::Baud9600;
    #[cfg(target_os = "windows")]
    return BaudRate::Baud9600;
    //return BaudRate::Baud115200;
}

#[cfg(feature = "novaview")]
fn create(port: &str, parameters: Parameters) -> anyhow::Result<E32Module> {
    use linux_embedded_hal::gpio_cdev::Chip;
    use std::path::PathBuf;

    let baud_rate = modem_baud_rate();
    let stop_bits = StopBits::Stop1;
    let comms_settings: PortSettings = PortSettings {
        baud_rate,
        char_size: CharSize::Bits8,
        parity: Parity::ParityNone,
        stop_bits,
        flow_control: FlowControl::FlowNone,
    };
    let mut config_settings = comms_settings.clone();
    config_settings.baud_rate = BaudRate::Baud9600;

    let mut port = ::serial::open(port)?;
    debug!("Serial port openend");
    port.configure(&config_settings)?;
    port.set_timeout(ANSWER_TIMEOUT)?;
    let serial = Serial::new(Rc::new(RefCell::new(port)));
    let mut chip = Chip::new::<PathBuf>("/dev/gpiochip0".into())?;
    debug!("GPIO opened");
    let aux = CtsAux::new(&mut chip)?;
    debug!("GPIO aux allocated");
    let m0 = M0Dtr::new(&mut chip)?;
    debug!("GPIO M0 allocated");
    let m1 = M1Rts::new(&mut chip)?;
    debug!("GPIO M1 allocated");
    let delay = StandardDelay {};
    debug!("GPIO lines allocated");
    let mut module = Ebyte::new(serial, aux, m0, m1, delay)?;
    debug!("Created module");
    configure(&mut module, &parameters)?;
    Ok(module)
}

#[cfg(not(feature = "novaview"))]
fn create(port: &str, parameters: Parameters) -> anyhow::Result<E32Module> {
    let baud_rate = modem_baud_rate();
    let stop_bits = StopBits::Stop1;

    let comms_settings: PortSettings = PortSettings {
        baud_rate,
        char_size: CharSize::Bits8,
        parity: Parity::ParityNone,
        stop_bits,
        flow_control: FlowControl::FlowNone,
    };
    let config_settings = comms_settings.clone();

    let mut port = ::serial::open(port)?;
    port.configure(&config_settings)?;
    port.set_timeout(ANSWER_TIMEOUT)?;

    let port = Rc::new(RefCell::new(port));
    let serial = Serial::new(port.clone());

    let aux = CtsAux::new(port.clone());

    let m0 = M0Dtr::new(port.clone());
    let m1 = M1Rts::new(port.clone());
    let delay = StandardDelay {};
    //serial.configure(&comms_settings);
    let mut module = Ebyte::new(serial, aux, m0, m1, delay)?;
    #[cfg(not(target_os = "windows"))]
    configure(&mut module, &parameters)?;

    Ok(module)
}

fn configure(module: &mut E32Module, parameters: &Parameters) -> anyhow::Result<()> {
    // Cargo-cultish, but it appears I need to read once first.
    debug!("before parameters read");
    let _ = module.parameters()?;
    debug!("parameters read");
    for i in 0..10 {
        module.set_parameters(parameters, ebyte_e32::parameters::Persistence::Permanent)?;
        let active = module.parameters()?;
        if active == *parameters {
            info!("Successfully configured E32Module");
            return Ok(());
        }
        error!("Parameters not set successfully, retrying {}", i);
        error!("active: {:?}, wanted: {:?}", active, *parameters);
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(anyhow!("Can't configure module!"))
}
