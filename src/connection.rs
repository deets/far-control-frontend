#[cfg(feature = "test-stand")]
use crate::observables::rqa as rqobs;

#[cfg(feature = "rocket")]
use crate::observables::rqb as rqobs;

use rqobs::RawObservablesGroup;

#[derive(Debug, PartialEq)]
pub enum Answers {
    Received(Vec<u8>),
    Observables(RawObservablesGroup),
    Timeout,
    ConnectionOpen,
    ConnectionError,
    Drained,
}

pub trait Connection: std::io::Write {
    fn recv(&mut self, callback: impl FnOnce(Answers));
    fn drain(&mut self);
    fn open(&mut self, port: &str);
    fn reset(&mut self);
    fn resume(&mut self);
    fn radio_silence(&mut self, radio_silence: bool);
}
