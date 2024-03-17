use crate::observables::rqa::RawObservablesGroup;

#[derive(Debug, PartialEq)]
pub enum Answers {
    Received(Vec<u8>),
    Observables(RawObservablesGroup),
    Timeout,
    ConnectionError,
}

pub trait Connection: std::io::Write {
    fn recv(&mut self, callback: impl FnOnce(Answers));
}
