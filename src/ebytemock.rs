use std::{
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};

use crate::{
    connection::{Answers, Connection},
    rqparser::{command_parser, MAX_BUFFER_SIZE},
};

enum Command {
    Send(Vec<u8>),
    Quit,
}

pub struct E32Connection {
    command_sender: Sender<Command>,
    response_receiver: Receiver<Answers>,
    worker: Option<JoinHandle<()>>,
}

impl Connection for E32Connection {
    fn recv(&mut self, callback: impl FnOnce(Answers)) {
        match self.response_receiver.try_recv() {
            Ok(answer) => {
                callback(answer);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                panic!("Crossbeam channel to ebyte module disconnected!");
            }
        }
    }
}

impl std::io::Write for E32Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.command_sender
            .send(Command::Send(buf.into()))
            .expect("crossbeam not working");
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct MockWorker {
    command_receiver: Receiver<Command>,
    response_sender: Sender<Answers>,
}

impl E32Connection {
    pub fn new(_port: &str) -> anyhow::Result<E32Connection> {
        let (command_sender, command_receiver) = unbounded::<Command>();
        let (response_sender, response_receiver) = unbounded::<Answers>();

        let handle = thread::spawn(move || {
            let mut worker = MockWorker {
                command_receiver,
                response_sender,
            };
            worker.work();
        });

        Ok(Self {
            command_sender,
            response_receiver,
            worker: Some(handle),
        })
    }

    fn quit(&mut self) {
        self.command_sender.send(Command::Quit).expect("crossbeam");
        // See https://stackoverflow.com/questions/57670145/how-to-store-joinhandle-of-a-thread-to-close-it-later
        self.worker.take().map(JoinHandle::join);
    }
}

impl Drop for E32Connection {
    fn drop(&mut self) {
        self.quit();
    }
}

impl MockWorker {
    fn work(&mut self) {
        loop {
            match self.command_receiver.recv() {
                Ok(m) => match m {
                    Command::Send(data) => self.process_data(&data),
                    Command::Quit => break,
                },
                Err(_) => panic!("Crossbeam is angry"),
            }
        }
    }

    fn process_data(&mut self, data: &Vec<u8>) {
        match command_parser(&data[1..data.len() - 4]) {
            Ok((.., transaction)) => {
                std::thread::sleep(Duration::from_millis(2000));
                let mut buffer = [0; MAX_BUFFER_SIZE];
                let response = transaction.acknowledge(&mut buffer).expect("must work");
                self.response_sender
                    .send(Answers::Received(response.into()))
                    .expect("cb angry");
            }
            Err(_) => unreachable!("We should never receive wrong commands"),
        }
    }
}
