use anyhow::anyhow;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use ebyte_e32::{mode::Normal, Ebyte, Parameters};
use ebyte_e32_ftdi::{CtsAux, M0Dtr, M1Rts, Serial, StandardDelay};
use embedded_hal::serial::Read;
use log::{debug, error, warn};
use nb::block;
use serial_core::{BaudRate, CharSize, FlowControl, Parity, PortSettings, SerialPort, StopBits};
use std::{
    cell::RefCell,
    rc::Rc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    connection::{Answers, Connection},
    rqparser::{SentenceParser, MAX_BUFFER_SIZE},
    rqprotocol::{Command, Node, Response, Transaction},
};

const ANSWER_TIMEOUT: Duration = Duration::from_millis(100);

pub type E32Module = Ebyte<Serial, CtsAux, M0Dtr, M1Rts, StandardDelay, Normal>;

#[derive(Debug, PartialEq)]
enum Commands {
    Open(String),
    Send(Vec<u8>),
    Drain,
    Quit,
}

struct E32Worker<Id> {
    command_receiver: Receiver<Commands>,
    response_sender: Sender<Answers>,
    command_id_generator: Id,
    me: Node,
    target_red_queen: Node,
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
}

impl Drop for E32Connection {
    fn drop(&mut self) {
        self.quit();
    }
}

impl<Id> E32Worker<Id>
where
    Id: Iterator<Item = usize>,
{
    fn work(&mut self) {
        let mut module = None;
        loop {
            match self
                .command_receiver
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(m) => match m {
                    Commands::Quit => {
                        break;
                    }
                    Commands::Open(port) => {
                        if let Ok(m) = create(&port, default_parameters()) {
                            module = Some(m);
                            self.response_sender.send(Answers::ConnectionOpen).unwrap();
                        } else {
                            error!("Can't open port '{}'", port);
                        }
                    }
                    Commands::Send(data) => match &mut module {
                        Some(module) => {
                            debug!("sending {}", std::str::from_utf8(&data).unwrap());
                            module.write_buffer(&data).expect("can't send data");
                            if Self::receive_sentence_or_timeout(module, |sentence| {
                                debug!("sending data into main thread");
                                self.response_sender
                                    .send(Answers::Received(sentence.clone()))
                                    .expect("can't ack data");
                            }) {
                                self.send_timeout();
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
                    if let Some(module) = &mut module {
                        self.fetch_observables(module);
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
            let _ = block!(module.read());
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
        if Self::receive_sentence_or_timeout(module, |sentence| {
            match t.process_response(sentence) {
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
            }
        }) {
            debug!("timeout getting OBG{} data", obg);
            self.send_timeout();
        } else {
            // now the ack is supposed to happen
            if Self::receive_sentence_or_timeout(module, |sentence| {
                let _ = t.process_response(sentence);
            }) {
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
    ) -> bool {
        let last_comm = Instant::now();
        let mut count = 0;
        let mut sentence_parser = SentenceParser::new();

        loop {
            match block!(module.read()) {
                Ok(b) => {
                    let mut sentence: Option<Vec<u8>> = None;
                    // debug!("rx: {}", b as char);
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
                        debug!("{:?}", last_comm.elapsed());
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
        air_rate: ebyte_e32::parameters::AirBaudRate::Bps19200,
        transmission_mode: ebyte_e32::parameters::TransmissionMode::Transparent,
        io_drive_mode: ebyte_e32::parameters::IoDriveMode::PushPull,
        wakeup_time: ebyte_e32::parameters::WakeupTime::Ms250,
        fec: ebyte_e32::parameters::ForwardErrorCorrectionMode::On,
        transmission_power: ebyte_e32::parameters::TransmissionPower::Dbm21,
    }
}

#[cfg(target_os = "windows")]
fn modem_baud_rate() -> BaudRate {
    BaudRate::Baud115200
}

#[cfg(not(target_os = "windows"))]
fn modem_baud_rate() -> BaudRate {
    BaudRate::Baud9600
}

fn create(port: &str, parameters: Parameters) -> anyhow::Result<E32Module> {
    let baud_rate = modem_baud_rate();
    let stop_bits = StopBits::Stop1;

    let settings: PortSettings = PortSettings {
        baud_rate,
        char_size: CharSize::Bits8,
        parity: Parity::ParityNone,
        stop_bits,
        flow_control: FlowControl::FlowNone,
    };

    let mut port = ::serial::open(port)?;

    port.set_timeout(ANSWER_TIMEOUT)?;
    port.configure(&settings)?;

    let port = Rc::new(RefCell::new(port));
    let serial = Serial::new(port.clone());

    let aux = CtsAux::new(port.clone());

    let m0 = M0Dtr::new(port.clone());
    let m1 = M1Rts::new(port.clone());
    let delay = StandardDelay {};
    let mut module = Ebyte::new(serial, aux, m0, m1, delay)?;
    #[cfg(not(target_os = "windows"))]
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
