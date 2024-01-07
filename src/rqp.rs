use ringbuffer::RingBuffer;

const START_DELIMITER: u8 = b'$';
const CHECKSUM_DELIMITER: u8 = b'*';
const CR: u8 = b'\r';
const LF: u8 = b'\n';
const MAX_BUFFER_SIZE: usize = 82; // NMEA standard size!

#[derive(Debug, PartialEq)]
pub enum Error {
    OutputBufferOverflow,
}

enum State {
    WaitForStart,
    WaitForCR,
    WaitForLF,
}

struct SentenceParser<'a, RB> {
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
                        if size > self.output_buffer.len() {
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

fn nibble_to_hex(nibble: u8) -> u8 {
    match nibble {
        0..10 => nibble + 48, // ascii 0
        _ => nibble + 55,     // ascii A - 10
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nom::{
        bytes::complete::{tag, take_while_m_n},
        character::{is_digit, is_hex_digit},
        multi::many1,
        sequence::separated_pair,
        IResult,
    };

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

    fn hex_byte(s: &[u8]) -> IResult<&[u8], u8> {
        let (rest, out) = take_while_m_n(2, 2, is_hex_digit)(s)?;
        Ok((rest, unhex(out[0]).unwrap() << 4 | unhex(out[1]).unwrap()))
    }

    fn timestamp_unit(s: &[u8]) -> IResult<&[u8], u8> {
        let (rest, out) = take_while_m_n(2, 2, is_digit)(s)?;
        Ok((rest, (out[0] - 48) * 10 + out[1] - 48))
    }

    fn timestamp_prefix(s: &[u8]) -> IResult<&[u8], Vec<u8>> {
        many1(timestamp_unit)(s)
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

    fn timestamp(s: &[u8]) -> IResult<&[u8], (Vec<u8>, Duration)> {
        separated_pair(timestamp_prefix, tag(b"."), timestamp_suffix)(s)
    }

    #[test]
    fn test_timestamp_parsing() {
        assert_eq!(timestamp_unit(b"123456"), Ok((&b"3456"[..], 12)));
        assert_eq!(
            timestamp_prefix(b"123456"),
            Ok((b"".as_slice(), vec![12, 34, 56]))
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
            timestamp(b"123456.1"),
            Ok((
                b"".as_slice(),
                (vec![12, 34, 56], Duration::from_micros(100000))
            ))
        );
    }
}
