use ringbuffer::RingBuffer;

const START_DELIMITER: u8 = b'$';
const CR: u8 = b'\r';
const LF: u8 = b'\n';

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
    output_buffer: [u8; 82], // NMEA standard size!
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

#[cfg(test)]
mod tests {
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
}
