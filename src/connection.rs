#[derive(Debug, PartialEq)]
pub enum Answers {
    Received(Vec<u8>),
    Timeout,
    ConnectionError,
}

pub trait Connection: std::io::Write {
    fn recv(&mut self, callback: impl FnOnce(Answers));
}
