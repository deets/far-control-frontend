use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct RqTimestamp {
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub seconds: u8,
    pub fractional: Duration,
}

#[derive(Debug, PartialEq)]
pub enum Node {
    RedQueen(u8),  // RQ<X>
    Farduino(u8),  // FD<X>
    LaunchControl, // LNC
}
