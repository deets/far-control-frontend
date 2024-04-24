use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    thread::{self, JoinHandle},
};

use chrono::{DateTime, Utc};
use crossbeam_channel::{unbounded, Receiver, Sender};
use log::info;
enum Commands {
    Store(u8),
    Quit,
}

pub struct Recorder {
    worker: Option<JoinHandle<()>>,
    command_sender: Sender<Commands>,
    pub path: Option<PathBuf>,
}

impl Recorder {
    pub fn new(path: Option<PathBuf>) -> Self {
        let (command_sender, command_receiver) = unbounded::<Commands>();
        let path_copy = path.clone();
        let handle = thread::spawn(move || {
            if let Some(path) = path_copy {
                work_for_real(path, command_receiver);
            } else {
                loop {
                    match command_receiver.recv().unwrap() {
                        Commands::Quit => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        });
        Recorder {
            worker: Some(handle),
            command_sender,
            path,
        }
    }

    pub fn new_with_default_file() -> Self {
        let current_utc: DateTime<Utc> = Utc::now();
        let rfc_format: String = current_utc.format("%Y-%m-%d_%H-%M").to_string();
        let mut path = PathBuf::new();
        path.push(format!("{}-rqa.log", rfc_format));
        info!("Recording data to {:?}", path);
        Self::new(Some(path))
    }

    pub fn store(&mut self, c: u8) {
        self.command_sender.send(Commands::Store(c)).unwrap();
    }

    pub fn write_buffer(&mut self, buffer: &Vec<u8>) {
        for c in buffer {
            self.store(*c);
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.command_sender.send(Commands::Quit).unwrap();
        self.worker.take().map(JoinHandle::join);
    }
}

fn work_for_real(path: PathBuf, receiver: Receiver<Commands>) {
    let mut output_file = File::create(path).unwrap();
    let mut buffer = vec![];
    loop {
        match receiver.recv().unwrap() {
            Commands::Store(c) => {
                buffer.push(c);
                if buffer.len() > 1024 {
                    output_file.write_all(&buffer).unwrap();
                    buffer.clear();
                }
            }
            Commands::Quit => {
                if buffer.len() > 1024 {
                    output_file.write_all(&buffer).unwrap();
                    buffer.clear();
                }
                break;
            }
        }
    }
}
