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
}
