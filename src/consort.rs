use log::error;
#[cfg(test)]
use mock_instant::Instant;
#[cfg(not(test))]
use std::time::Instant;

use std::io::Write;

use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::{
    rqparser::{NMEAFormatError, SentenceParser},
    rqprotocol::{Command, Node, Response, Transaction, TransactionState},
};

use crate::rqparser::Error as ParserError;
use crate::rqprotocol::Error as ProtocolError;

#[derive(Debug, PartialEq)]
pub enum Error {
    ActiveTransaction,
    NMEAFormatError,
    ProtocolError,
    IOError,
    SpuriousSentence,
    ParserError,
}

// Liaison to the RedQueen2
#[derive(Debug)]
pub struct Consort<Id> {
    me: Node,
    dest: Node,
    sentence_parser: SentenceParser,
    transaction: Option<Transaction>,
    command_id_generator: Id,
    now: Instant,
}

impl From<NMEAFormatError<'_>> for Error {
    fn from(_value: NMEAFormatError) -> Self {
        Error::NMEAFormatError
    }
}

impl From<ProtocolError> for Error {
    fn from(value: ProtocolError) -> Self {
        error!("ProtocolError: {:?}", value);
        Error::ProtocolError
    }
}

impl From<std::io::Error> for Error {
    fn from(_value: std::io::Error) -> Self {
        Error::IOError
    }
}

impl From<ParserError> for Error {
    fn from(_value: ParserError) -> Self {
        Error::NMEAFormatError
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for Error {}

pub struct SimpleIdGenerator {
    id: usize,
}

impl Default for SimpleIdGenerator {
    fn default() -> Self {
        Self {
            id: Default::default(),
        }
    }
}

impl Iterator for SimpleIdGenerator {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        self.id = (self.id + 1) % 1000;
        Some(self.id)
    }
}

impl<Id> Consort<Id>
where
    Id: Iterator<Item = usize>,
{
    pub fn new_with_id_generator(
        me: Node,
        dest: Node,
        now: Instant,
        command_id_generator: Id,
    ) -> Self {
        let sentence_parser = SentenceParser::new();
        Self {
            me,
            dest,
            sentence_parser,
            transaction: None,
            command_id_generator,
            now,
        }
    }

    pub fn send_command<W: Write>(
        &mut self,
        command: Command,
        writer: &mut W,
    ) -> Result<(), Error> {
        match self.transaction {
            Some(_) => Err(Error::ActiveTransaction),
            None => {
                let transaction =
                    Transaction::new(self.me.clone(), self.dest.clone(), self.next_id(), command);
                let mut dest: [u8; 82] = [0; 82];
                writer.write(transaction.commandeer(&mut dest)?)?;
                self.transaction = Some(transaction);
                Ok(())
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.transaction.is_some()
    }

    pub fn reset(&mut self) {
        self.transaction = None
    }

    pub fn feed(
        &mut self,
        ringbuffer: &mut AllocRingBuffer<u8>,
    ) -> Result<Option<Response>, Error> {
        // This is a bit  ugly but so far I have no better answer.
        // To keep the interface based on a single Response (or None),
        // we feed the incoming data byte-wise into the system and
        // stop in the moment we get a full sentence.
        // This allows the call-site to spoon-feed the data and
        // react on the outgoing response, driving FSMs etc.
        let mut extracted_sentence: Option<Vec<u8>> = None;
        while !ringbuffer.is_empty() {
            let data = [ringbuffer.dequeue().unwrap()];
            self.sentence_parser.feed(&data, |sentence: &[u8]| {
                extracted_sentence = Some(sentence.into());
            })?;
            if let Some(_) = extracted_sentence {
                break;
            }
        }
        // if we extracted a sentence, process it
        if let Some(sentence) = extracted_sentence {
            match &mut self.transaction {
                Some(transaction) => {
                    let result = Ok(Some(transaction.process_response(sentence.as_slice())?));
                    if transaction.state() == TransactionState::Dead {
                        self.transaction = None;
                    }
                    return result;
                }
                // We don't expect data
                None => {
                    return Err(Error::SpuriousSentence);
                }
            }
        }
        Ok(None)
    }

    pub fn update_time(&mut self, now: Instant) {
        self.now = now;
    }

    fn next_id(&mut self) -> usize {
        self.command_id_generator.next().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use ringbuffer::RingBuffer;
    use std::assert_matches::assert_matches;
    use std::{
        cell::RefCell,
        cmp::min,
        io::{Read, Write},
        rc::Rc,
    };

    use crate::observables::AdcGain;

    use super::*;

    struct MockPort {
        pub sent_messages: Rc<RefCell<Vec<Vec<u8>>>>,
        pub expected_reads: Rc<RefCell<Vec<Vec<u8>>>>,
    }

    impl Read for MockPort {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let message = self.expected_reads.borrow_mut().pop().unwrap();
            // This is a bit dirty as it assumes reading always
            // returns full sentences, which obviously isn't true.
            let len = min(buf.len(), message.len());
            for i in 0..len {
                buf[i] = message[i];
            }
            assert_eq!(len, message.len());
            Ok(len)
        }
    }

    impl Write for MockPort {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.sent_messages.borrow_mut().push(buf.into());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Default for MockPort {
        fn default() -> Self {
            Self {
                sent_messages: Default::default(),
                expected_reads: Default::default(),
            }
        }
    }

    #[test]
    fn test_instantiation() {
        let _consort = Consort::new_with_id_generator(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            Instant::now(),
            SimpleIdGenerator::default(),
        );
    }

    #[test]
    fn test_sending_command() {
        let mut consort = Consort::new_with_id_generator(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            Instant::now(),
            SimpleIdGenerator::default(),
        );
        let mut mock_port = MockPort::default();
        consort
            .send_command(Command::Reset(AdcGain::Gain1), &mut mock_port)
            .unwrap();
        assert_eq!(
            mock_port.sent_messages.borrow_mut().pop(),
            Some(b"$LNCCMD,001,RQA,RESET,01*2C\r\n".as_slice().into())
        );
        let mut inputbuffer = ringbuffer::AllocRingBuffer::new(256);
        for c in b"$RQAACK,001,LNC,01*56\r\n" {
            inputbuffer.push(*c);
        }
        assert_matches!(consort.feed(&mut inputbuffer), Ok(Some(_)));
        assert_matches!(consort.transaction, None);
        assert!(inputbuffer.is_empty());
    }

    #[test]
    fn test_sending_command_and_receiving_partial_answer() {
        let mut consort = Consort::new_with_id_generator(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            Instant::now(),
            SimpleIdGenerator::default(),
        );
        let mut mock_port = MockPort::default();
        consort
            .send_command(Command::Reset(AdcGain::Gain2), &mut mock_port)
            .unwrap();
        assert_eq!(
            mock_port.sent_messages.borrow_mut().pop(),
            Some(b"$LNCCMD,001,RQA,RESET,02*2F\r\n".as_slice().into())
        );
        let mut inputbuffer = ringbuffer::AllocRingBuffer::new(256);
        for c in b"$RQAACK,001" {
            inputbuffer.push(*c);
        }
        assert_matches!(consort.feed(&mut inputbuffer), Ok(None));
        assert_matches!(consort.transaction, Some(_));
    }

    #[test]
    fn test_sending_spurious_command() {
        let mut consort = Consort::new_with_id_generator(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            Instant::now(),
            SimpleIdGenerator::default(),
        );

        let mut inputbuffer = ringbuffer::AllocRingBuffer::new(256);
        for c in b"$RQAACK,123456.001,LNC,001*4F\r\n" {
            inputbuffer.push(*c);
        }
        assert_matches!(consort.feed(&mut inputbuffer), Err(Error::SpuriousSentence));
    }
}
