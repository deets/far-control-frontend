use crate::rqprotocol::{AckHeader, Acknowledgement, Node, RqTimestamp};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while_m_n},
    character::{complete::alphanumeric1, is_alphabetic, is_digit, is_hex_digit},
    multi::many1_count,
    sequence::{preceded, separated_pair, tuple},
    IResult,
};
use ringbuffer::RingBuffer;
use std::time::Duration;

const START_DELIMITER: u8 = b'$';
const CHECKSUM_DELIMITER: u8 = b'*';
const CR: u8 = b'\r';
const LF: u8 = b'\n';
const MAX_BUFFER_SIZE: usize = 82; // NMEA standard size!

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
pub struct SentenceParser<'a, RB> {
    state: State,
    ring_buffer: &'a mut RB,
    output_buffer: [u8; MAX_BUFFER_SIZE],
}

impl<'a, RB> SentenceParser<'a, RB>
where
    RB: RingBuffer<u8>,
{
    pub fn new(ring_buffer: &'a mut RB) -> Self {
        ring_buffer.clear();
        Self {
            state: State::WaitForStart,
            ring_buffer,
            output_buffer: [0; 82],
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

fn unhex_two_bytes(b: &[u8]) -> Result<u8, NMEAFormatError> {
    let (upper, lower) = (b[0], b[1]);
    Ok(unhex(upper)? << 4 | unhex(lower)?)
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

fn timestamp_unit(s: &[u8]) -> IResult<&[u8], u8> {
    let (rest, out) = take_while_m_n(2, 2, is_digit)(s)?;
    Ok((rest, (out[0] - 48) * 10 + out[1] - 48))
}

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
    let (rest, (source, acknowledgement, _, timestamp, _, recipient, _, id)) = tuple((
        node_parser,
        alt((tag(b"ACK"), tag(b"NAK"))),
        tag(b","),
        timestamp_parser,
        tag(b","),
        node_parser,
        tag(b","),
        command_id_parser,
    ))(s)?;
    let response = AckHeader {
        source,
        recipient,
        timestamp,
        id,
    };
    match acknowledgement {
        b"ACK" => Ok((rest, Acknowledgement::Ack(response))),
        b"NAK" => Ok((rest, Acknowledgement::Nak(response))),
        _ => unreachable!(),
    }
}

pub fn one_return_value_parser(s: &[u8]) -> IResult<&[u8], u8> {
    preceded(tag(b","), hex_byte)(s)
}

pub fn two_return_values_parser(s: &[u8]) -> IResult<&[u8], (u8, u8)> {
    tuple((one_return_value_parser, one_return_value_parser))(s)
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

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_feeding_full_sentence() {
        let sentence = b"$RQSTATE,013940.4184,DROGUE_OPEN*39\r\n";
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut parser = SentenceParser::new(&mut ringbuffer);
        let mut called = false;
        parser
            .feed(sentence, |output_sentence| {
                called = true;
                assert_eq!(output_sentence.len(), 37);
                assert_eq!(&output_sentence[0..37], sentence);
            })
            .expect("");
        assert!(called);
    }

    #[test]
    fn test_too_small_output_buffer() {
        let sentence = b"$RQSTATE,01234567890123456789012345678901234567890123456789012345678901234567890123456789013940.4184,DROGUE_OPEN*39\r\n";
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut parser = SentenceParser::new(&mut ringbuffer);
        assert_eq!(
            Err(Error::OutputBufferOverflow),
            parser.feed(sentence, |_| {})
        );
    }

    #[test]
    fn test_output_buffer_overvflow_recovers() {
        let sentence = b"$RQSTATE,01234567890123456789012345678901234567890123456789012345678901234567890123456789013940.4184,DROGUE_OPEN*39\r\n";
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut parser = SentenceParser::new(&mut ringbuffer);

        assert_eq!(
            Err(Error::OutputBufferOverflow),
            parser.feed(sentence, |sentence| {
                assert_eq!(sentence, b"$TEST\r\n");
            })
        );
        assert_matches!(
            parser.feed(b"$TEST\r\n", |sentence| {
                assert_eq!(sentence, b"$TEST\r\n");
            }),
            Ok(_),
        );
    }

    #[test]
    fn test_leading_garbage_is_discarded() {
        let sentence =
            b"prentend-this-is-an-earlier-sentence\r\n$RQSTATE,013940.4184,DROGUE_OPEN*39\r\n";
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut parser = SentenceParser::new(&mut ringbuffer);
        let mut called = false;
        parser
            .feed(sentence, |output_sentence| {
                called = true;
                assert_eq!(output_sentence.len(), 37);
            })
            .expect("");
        assert!(called);
    }

    #[test]
    fn test_even_more_garbage_is_discarded() {
        let sentence = b"$\rX\r\n$RQSTATE,013940.4184,DROGUE_OPEN*39\r\n";
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut parser = SentenceParser::new(&mut ringbuffer);
        let mut called = false;
        parser
            .feed(sentence, |output_sentence| {
                called = true;
                println!("{:?}", output_sentence);
                assert_eq!(output_sentence.len(), 37);
            })
            .expect("should've worked");
        assert!(called);
    }

    #[test]
    fn test_nmea_format_verification() {
        assert_eq!(Err(NMEAFormatError::FormatError), verify_nmea_format(b""));
        assert_eq!(Err(NMEAFormatError::FormatError), verify_nmea_format(b""));
        assert_eq!(
            Ok(b"PFEC,GPint,RMC05".as_slice()),
            verify_nmea_format(b"$PFEC,GPint,RMC05*2D\r\n")
        );
        assert_eq!(
            Err(NMEAFormatError::NoChecksumError(b"".as_slice())),
            verify_nmea_format(b"$\r\n")
        );

        assert_eq!(
            Err(NMEAFormatError::ChecksumError),
            verify_nmea_format(b"$PFEC,GPint,RMC05*2E\r\n")
        );
    }

    #[test]
    fn test_nmea_formatter() {
        let mut formatter = NMEAFormatter::default();
        formatter.format_sentence(b"PFEC,GPint,RMC05").unwrap();
        assert_eq!(
            Ok(b"$PFEC,GPint,RMC05*2D\r\n".as_slice()),
            formatter.buffer()
        );
    }

    #[test]
    fn test_timestamp_parsing() {
        assert_eq!(timestamp_unit(b"123456"), Ok((&b"3456"[..], 12)));
        assert_eq!(
            timestamp_prefix(b"123456"),
            Ok((b"".as_slice(), (Some(12), Some(34), 56)))
        );
        assert_eq!(
            timestamp_suffix(b"000001"),
            Ok((b"".as_slice(), Duration::from_micros(1)))
        );
        assert_eq!(
            timestamp_suffix(b"1"),
            Ok((b"".as_slice(), Duration::from_micros(100000)))
        );

        assert_eq!(
            timestamp_parser(b"123456.1"),
            Ok((
                b"".as_slice(),
                (RqTimestamp {
                    hour: Some(12),
                    minute: Some(34),
                    seconds: 56,
                    fractional: Duration::from_micros(100000)
                })
            ))
        );
        assert_eq!(
            timestamp_parser(b"3456.1"),
            Ok((
                b"".as_slice(),
                (RqTimestamp {
                    hour: None,
                    minute: Some(34),
                    seconds: 56,
                    fractional: Duration::from_micros(100000)
                })
            ))
        );
        assert_eq!(
            timestamp_parser(b"56.1"),
            Ok((
                b"".as_slice(),
                (RqTimestamp {
                    hour: None,
                    minute: None,
                    seconds: 56,
                    fractional: Duration::from_micros(100000)
                })
            ))
        );
    }

    #[test]
    fn test_node_parsing() {
        assert_eq!(
            node_parser(b"RQA"),
            Ok((b"".as_slice(), Node::RedQueen(b'A')),)
        );
        assert_eq!(
            node_parser(b"RQB"),
            Ok((b"".as_slice(), Node::RedQueen(b'B')),)
        );
        assert_eq!(
            node_parser(b"FDC"),
            Ok((b"".as_slice(), Node::Farduino(b'C')),)
        );
        assert_eq!(
            node_parser(b"LNC"),
            Ok((b"".as_slice(), Node::LaunchControl,))
        );
        assert_eq!(
            node_parser(b"RQBFOO"),
            Ok((b"FOO".as_slice(), Node::RedQueen(b'B')),)
        );
    }

    #[test]
    fn test_command_id_parser() {
        assert_matches!(command_id_parser(b"123"), Ok((_, 123)));
    }

    #[test]
    fn test_ack_parsing() {
        let inner_sentence = b"RQEACK,123456.012,LNC,123";
        assert_matches!(
            ack_parser(inner_sentence),
            Ok((
                _,
                Acknowledgement::Ack(AckHeader {
                    recipient: Node::LaunchControl,
                    source: Node::RedQueen(b'E'),
                    id: 123,
                    ..
                })
            ))
        );
        let inner_sentence = b"RQENAK,123456.012,LNC,123";
        assert_matches!(ack_parser(inner_sentence), Ok((_, Acknowledgement::Nak(_))));
    }

    #[test]
    fn test_return_argument_parsing() {
        assert_matches!(one_return_value_parser(b",3F"), Ok((b"", 0x3f)));
        assert_matches!(two_return_values_parser(b",AB,CD"), Ok((b"", (0xab, 0xcd))));
    }
}
