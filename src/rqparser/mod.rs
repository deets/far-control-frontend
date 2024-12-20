use crate::{
    observables::AdcGain,
    rqprotocol::{AckHeader, Acknowledgement, Command, Node, RqTimestamp, Transaction},
};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till, take_while_m_n},
    character::{is_alphabetic, is_digit, is_hex_digit},
    multi::many1_count,
    sequence::{preceded, separated_pair, tuple},
    IResult,
};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use std::time::Duration;

#[cfg(feature = "test-stand")]
pub mod rqa;
#[cfg(feature = "rocket")]
pub mod rqb;

const START_DELIMITER: u8 = b'$';
const CHECKSUM_DELIMITER: u8 = b'*';
const CR: u8 = b'\r';
const LF: u8 = b'\n';
pub const MAX_BUFFER_SIZE: usize = 82; // NMEA standard size!

#[derive(Debug, PartialEq)]
pub enum Error {
    OutputBufferOverflow,
}

#[derive(Debug)]
enum State {
    WaitForStart,
    WaitForCR,
    WaitForLF,
}

#[derive(Debug)]
pub struct SentenceParser {
    state: State,
    ring_buffer: AllocRingBuffer<u8>,
    output_buffer: [u8; MAX_BUFFER_SIZE],
}

impl SentenceParser {
    pub fn new() -> Self {
        Self {
            state: State::WaitForStart,
            ring_buffer: AllocRingBuffer::new(256),
            output_buffer: [0; MAX_BUFFER_SIZE],
        }
    }

    pub fn feed(&mut self, data: &[u8], mut process: impl FnMut(&[u8])) -> Result<(), Error> {
        for c in data {
            let c = *c;
            match self.state {
                State::WaitForStart => {
                    if c == START_DELIMITER {
                        self.state = State::WaitForCR;
                        self.ring_buffer.push(c);
                    }
                }
                State::WaitForCR => {
                    self.ring_buffer.push(c);
                    if c == CR {
                        self.state = State::WaitForLF
                    }
                }
                State::WaitForLF => {
                    if c == LF {
                        self.ring_buffer.push(c);
                        self.state = State::WaitForStart;
                        let size = self.ring_buffer.len();
                        // The contained sentence is beyond our
                        // buffer size. We must discard.
                        if size > self.output_buffer.len() {
                            self.ring_buffer.clear();
                            return Err(Error::OutputBufferOverflow);
                        }
                        for (index, char) in self.ring_buffer.drain().enumerate() {
                            self.output_buffer[index] = char;
                        }
                        process(&self.output_buffer[0..size]);
                    } else {
                        // Our violated expectation just means
                        // we discard and reset
                        self.ring_buffer.clear();
                        self.state = State::WaitForStart;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NMEAFormatError<'a> {
    FormatError,
    NoChecksumError(&'a [u8]),
    ChecksumError,
    SentenceTooLongError,
    NoSentenceAvailable,
}

fn unhex<'a>(c: u8) -> Result<u8, NMEAFormatError<'a>> {
    // I currently assume NMEA specifies upper case hex only
    match c {
        b'0'..=b'9' => Ok(c - 48),
        b'A'..=b'F' => Ok(c - 55),
        _ => Err(NMEAFormatError::ChecksumError),
    }
}

fn verify_nmea_checksum<'a>(
    message: &'a [u8],
    upper: u8,
    lower: u8,
) -> Result<&'a [u8], NMEAFormatError> {
    let checksum = unhex(upper)? << 4 | unhex(lower)?;
    let mut running = 0;
    for c in message {
        running = running ^ c;
    }
    if running == checksum {
        Ok(message)
    } else {
        Err(NMEAFormatError::ChecksumError)
    }
}

fn verify_inner_nmea<'a>(message: &'a [u8]) -> Result<&'a [u8], NMEAFormatError> {
    // messages without checksum are valid, but we return them as error containing the
    // reference.
    match message.len() {
        0..2 => Err(NMEAFormatError::NoChecksumError(message)),
        n => {
            // the checksum is a 2-digit-hexadecimal ASCII string prefixed
            // by *, so let's check for that
            match message[n - 3] {
                b'*' => verify_nmea_checksum(&message[0..n - 3], message[n - 2], message[n - 1]),
                _ => Err(NMEAFormatError::NoChecksumError(message)),
            }
        }
    }
}

pub fn verify_nmea_format<'a>(message: &'a [u8]) -> Result<&'a [u8], NMEAFormatError> {
    match message.len() {
        0..2 => Err(NMEAFormatError::FormatError),
        n => {
            if message[0] == START_DELIMITER && message[n - 1] == LF && message[n - 2] == CR {
                return verify_inner_nmea(&message[1..n - 2]);
            }
            return Err(NMEAFormatError::FormatError);
        }
    }
}

pub struct NMEAFormatter {
    buffer: [u8; MAX_BUFFER_SIZE],
    len: Option<usize>,
}

impl Default for NMEAFormatter {
    fn default() -> Self {
        Self {
            buffer: [0; MAX_BUFFER_SIZE],
            len: None,
        }
    }
}

impl NMEAFormatter {
    pub fn buffer(&self) -> Result<&[u8], NMEAFormatError> {
        Ok(&self.buffer[0..self.len.ok_or(NMEAFormatError::NoSentenceAvailable)?])
    }

    pub fn format_sentence(&mut self, output: &[u8]) -> Result<(), NMEAFormatError> {
        match output.len() {
            d if d > (MAX_BUFFER_SIZE - 6) => Err(NMEAFormatError::SentenceTooLongError),
            _ => {
                self.buffer[0] = START_DELIMITER;
                let mut checksum = 0;
                for (index, char) in output.into_iter().enumerate() {
                    self.buffer[1 + index] = *char;
                    checksum = checksum ^ *char;
                }
                self.buffer[output.len() + 1] = CHECKSUM_DELIMITER;
                (self.buffer[output.len() + 2], self.buffer[output.len() + 3]) =
                    (nibble_to_hex(checksum >> 4), nibble_to_hex(checksum & 0x0f));
                self.buffer[output.len() + 4] = CR;
                self.buffer[output.len() + 5] = LF;
                self.len = Some(output.len() + 6);
                Ok(())
            }
        }
    }
}

pub fn nibble_to_hex(nibble: u8) -> u8 {
    match nibble {
        0..10 => nibble + 48, // ascii 0
        _ => nibble + 55,     // ascii A - 10
    }
}

#[allow(dead_code)]
fn timestamp_unit(s: &[u8]) -> IResult<&[u8], u8> {
    let (rest, out) = take_while_m_n(2, 2, is_digit)(s)?;
    Ok((rest, (out[0] - 48) * 10 + out[1] - 48))
}

#[allow(dead_code)]
fn timestamp_prefix(s: &[u8]) -> IResult<&[u8], (Option<u8>, Option<u8>, u8)> {
    let (rest, count) = many1_count(timestamp_unit)(s)?;
    let prefix = &s[0..count * 2];
    let (mut hour, mut minute) = (None, None);
    let mut seconds = 0;
    match count {
        3 => {
            let (_, (h, m, s)) = tuple((timestamp_unit, timestamp_unit, timestamp_unit))(prefix)?;
            (hour, minute) = (Some(h), Some(m));
            seconds = s;
        }
        2 => {
            let (_, (m, s)) = tuple((timestamp_unit, timestamp_unit))(prefix)?;
            minute = Some(m);
            seconds = s;
        }
        1 => {
            let (_, s) = timestamp_unit(prefix)?;
            seconds = s;
        }
        _ => unreachable!(),
    }
    Ok((rest, (hour, minute, seconds)))
}

fn usize_parser(s: &[u8]) -> IResult<&[u8], usize> {
    let (rest, out) = take_while_m_n(1, 8, is_digit)(s)?;
    let mut accu: usize = 0;
    for c in out {
        accu *= 10;
        accu += (*c - 48) as usize;
    }
    Ok((rest, accu))
}

fn timestamp_suffix(s: &[u8]) -> IResult<&[u8], Duration> {
    let (rest, out) = take_while_m_n(1, 6, is_digit)(s)?;
    let mut accu: u64 = 0;
    for c in out {
        accu *= 10;
        accu += (*c - 48) as u64;
    }
    // We need to multiply out what we are missing to a
    // microsecond value
    for _ in 0..(6 - out.len()) {
        accu *= 10;
    }
    Ok((rest, Duration::from_micros(accu)))
}

fn timestamp_parser(s: &[u8]) -> IResult<&[u8], RqTimestamp> {
    let (rest, (prefix, fractional)) =
        separated_pair(timestamp_prefix, tag(b"."), timestamp_suffix)(s)?;
    Ok((
        rest,
        RqTimestamp {
            hour: prefix.0,
            minute: prefix.1,
            seconds: prefix.2,
            fractional,
        },
    ))
}

pub fn ack_parser(s: &[u8]) -> IResult<&[u8], Acknowledgement> {
    let (rest, (source, acknowledgement, _, id, _, recipient)) = tuple((
        node_parser,
        alt((tag(b"ACK"), tag(b"NAK"))),
        tag(b","),
        command_id_parser,
        tag(b","),
        node_parser,
    ))(s)?;
    let response = AckHeader {
        source,
        recipient,
        id,
    };
    match acknowledgement {
        b"ACK" => Ok((rest, Acknowledgement::Ack(response))),
        b"NAK" => Ok((rest, Acknowledgement::Nak(response))),
        _ => unreachable!(),
    }
}

pub fn one_hex_return_value_parser(s: &[u8]) -> IResult<&[u8], u8> {
    preceded(tag(b","), hex_byte)(s)
}

pub fn two_return_values_parser(s: &[u8]) -> IResult<&[u8], (u8, u8)> {
    tuple((one_hex_return_value_parser, one_hex_return_value_parser))(s)
}

pub fn one_usize_return_value_parser(s: &[u8]) -> IResult<&[u8], usize> {
    preceded(tag(b","), usize_parser)(s)
}

fn hex_byte(s: &[u8]) -> IResult<&[u8], u8> {
    let (rest, out) = take_while_m_n(2, 2, is_hex_digit)(s)?;
    Ok((rest, unhex(out[0]).unwrap() << 4 | unhex(out[1]).unwrap()))
}

fn avionics_parser(s: &[u8]) -> IResult<&[u8], Node> {
    let (rest, (praefix, identifier)) = tuple((
        alt((tag(b"RQ"), tag(b"FD"))),
        take_while_m_n(1, 1, is_alphabetic),
    ))(s)?;
    match praefix {
        b"RQ" => Ok((rest, Node::RedQueen(identifier[0]))),
        b"FD" => Ok((rest, Node::Farduino(identifier[0]))),
        _ => unreachable!(),
    }
}

fn lnc_parser(s: &[u8]) -> IResult<&[u8], Node> {
    let (rest, _) = tag(b"LNC")(s)?;
    Ok((rest, Node::LaunchControl))
}

fn node_parser(s: &[u8]) -> IResult<&[u8], Node> {
    alt((lnc_parser, avionics_parser))(s)
}

fn command_id_parser(s: &[u8]) -> IResult<&[u8], usize> {
    let (rest, bytes) = take_while_m_n(3, 3, is_digit)(s)?;
    let a = (bytes[0] - b'0') as usize;
    let b = (bytes[1] - b'0') as usize;
    let c = (bytes[2] - b'0') as usize;
    Ok((rest, (a * 100 + b * 10 + c)))
}

fn command_prefix_parser(s: &[u8]) -> IResult<&[u8], (Node, usize, Node)> {
    // LNCCMD,123,RQA
    let (rest, (source, _, command_id, _, recipient, _)) = tuple((
        node_parser,
        tag(b"CMD,"),
        command_id_parser,
        tag(","),
        node_parser,
        tag(b","),
    ))(s)?;
    Ok((rest, (source, command_id, recipient)))
}

fn gain_parser(s: &[u8]) -> IResult<&[u8], AdcGain> {
    let (rest, num) = hex_u8_parser(s)?;
    let gain = match num {
        1 => AdcGain::Gain1,
        2 => AdcGain::Gain2,
        4 => AdcGain::Gain4,
        8 => AdcGain::Gain8,
        16 => AdcGain::Gain16,
        32 => AdcGain::Gain32,
        64 => AdcGain::Gain64,
        _ => unreachable!(),
    };
    Ok((rest, gain))
}

fn command_reset_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,RESET,40
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, (_, _, gain)) = tuple((tag(b"RESET"), tag(","), gain_parser))(rest)?;
    let transaction = Transaction::new(source, recipient, command_id, Command::Reset(gain));
    Ok((rest, transaction))
}

fn command_ping_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,PING
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, _) = tag(b"PING")(rest)?;
    let transaction = Transaction::new(source, recipient, command_id, Command::Ping);
    Ok((rest, transaction))
}

fn command_ignition_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,IGNITION
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, _) = tag(b"IGNITION")(rest)?;
    let transaction = Transaction::new(source, recipient, command_id, Command::Ignition);
    Ok((rest, transaction))
}

fn command_unlock_pyros_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,UNLOCK_PYROS
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, _) = tag(b"UNLOCK_PYROS")(rest)?;
    let transaction = Transaction::new(source, recipient, command_id, Command::UnlockPyros);
    Ok((rest, transaction))
}

fn command_secret_partial_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,SECRET_A,3F
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, (_, secret)) = tuple((tag(b"SECRET_A,"), hex_byte))(rest)?;
    let transaction = Transaction::new(
        source,
        recipient,
        command_id,
        Command::LaunchSecretPartial(secret),
    );
    Ok((rest, transaction))
}

fn command_obg_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,OBG,01
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, (_, group)) = tuple((tag(b"OBG,"), usize_parser))(rest)?;
    let transaction = Transaction::new(
        source,
        recipient,
        command_id,
        Command::ObservableGroup(group),
    );
    Ok((rest, transaction))
}

fn command_secret_full_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    // LNCCMD,123,RQA,SECRET_AB,3F,AB
    let (rest, (source, command_id, recipient)) = command_prefix_parser(s)?;
    let (rest, (_, secret_a, _, secret_b)) =
        tuple((tag(b"SECRET_AB,"), hex_byte, tag(b","), hex_byte))(rest)?;
    let transaction = Transaction::new(
        source,
        recipient,
        command_id,
        Command::LaunchSecretFull(secret_a, secret_b),
    );
    Ok((rest, transaction))
}

pub fn command_parser(s: &[u8]) -> IResult<&[u8], Transaction> {
    alt((
        command_reset_parser,
        command_ignition_parser,
        command_unlock_pyros_parser,
        command_secret_partial_parser,
        command_secret_full_parser,
        command_ping_parser,
        command_obg_parser,
    ))(s)
}

fn hex_u32_parser(s: &[u8]) -> IResult<&[u8], u32> {
    let (rest, out) = take_while_m_n(8, 8, is_hex_digit)(s)?;
    let mut res: u32 = 0;
    for i in 0..8 {
        res <<= 4;
        res |= unhex(out[i]).unwrap() as u32;
    }
    Ok((rest, res))
}

fn hex_u16_parser(s: &[u8]) -> IResult<&[u8], u16> {
    let (rest, out) = take_while_m_n(4, 4, is_hex_digit)(s)?;
    let mut res: u16 = 0;
    for i in 0..4 {
        res <<= 4;
        res |= unhex(out[i]).unwrap() as u16;
    }
    Ok((rest, res))
}

fn hex_u8_parser(s: &[u8]) -> IResult<&[u8], u8> {
    let (rest, out) = take_while_m_n(2, 2, is_hex_digit)(s)?;
    let mut res: u8 = 0;
    for i in 0..2 {
        res <<= 4;
        res |= unhex(out[i]).unwrap() as u8;
    }
    Ok((rest, res))
}

fn hex_i32_parser(s: &[u8]) -> IResult<&[u8], i32> {
    let (rest, num) = hex_u32_parser(s)?;
    Ok((rest, num as i32))
}

fn hex_u64_parser(s: &[u8]) -> IResult<&[u8], u64> {
    let (rest, out) = take_while_m_n(16, 16, is_hex_digit)(s)?;
    let mut res: u64 = 0;
    for i in 0..16 {
        res <<= 4;
        res |= unhex(out[i]).unwrap() as u64;
    }
    Ok((rest, res))
}

fn string_parser(s: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (rest, string) = take_till(|c| c == b'*' || c == b',')(s)?;
    Ok((rest, string.into()))
}
