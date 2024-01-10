use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct RqTimestamp {
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub seconds: u8,
    pub fractional: Duration,
}

#[derive(Debug, PartialEq)]
pub enum AvionicsModel {
    RedQueen,
    Farduino,
}

#[derive(Debug, PartialEq)]
pub struct Node {
    pub model: AvionicsModel,
    pub identifier: u8,
}
