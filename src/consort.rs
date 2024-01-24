use egui_glow::glow::ActiveTransformFeedback;
#[cfg(test)]
use mock_instant::Instant;
#[cfg(not(test))]
use std::time::Instant;

use std::io::Write;

use ringbuffer::AllocRingBuffer;

use crate::{
    rqparser::{NMEAFormatError, NMEAFormatter, SentenceParser},
    rqprotocol::{Command, Node, Response, Serialize, Transaction, TransactionState},
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
pub struct Consort<'a> {
    me: Node,
    dest: Node,
    sentence_parser: SentenceParser<'a, AllocRingBuffer<u8>>,
    transaction: Option<Transaction>,
    command_id: usize,
    now: Instant,
}

impl From<NMEAFormatError<'_>> for Error {
    fn from(_value: NMEAFormatError) -> Self {
        Error::NMEAFormatError
    }
}

impl From<ProtocolError> for Error {
    fn from(_value: ProtocolError) -> Self {
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

impl<'a> Consort<'a> {
    fn new(me: Node, dest: Node, ringbuffer: &'a mut AllocRingBuffer<u8>, now: Instant) -> Self {
        let sentence_parser = SentenceParser::new(ringbuffer);
        Self {
            me,
            dest,
            sentence_parser,
            transaction: None,
            command_id: 0, // TODO: randomize
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
                let remaining = transaction.serialize(&mut dest, 0..82)?;
                let mut formatter = NMEAFormatter::default();
                formatter.format_sentence(&dest[0..remaining.start])?;
                writer.write(formatter.buffer()?)?;
                self.transaction = Some(transaction);
                Ok(())
            }
        }
    }

    pub fn feed(
        &mut self,
        ringbuffer: &'a mut AllocRingBuffer<u8>,
    ) -> Result<Option<Response>, Error> {
        let mut extracted_sentence: Option<Vec<u8>> = None;
        // This is a bit  ugly but so far I have no better answer.
        // To keep the interface based on a single Response (or None),
        // we feed the incoming data byte-wise into the system and
        // stop in the moment we get a full sentence.
        // This allows the call-site to spoon-feed the data and
        // react on the outgoing response, driving FSMs etc.
        for c in ringbuffer {
            let data = [*c];
            self.sentence_parser.feed(&data, |sentence: &[u8]| {
                extracted_sentence = Some(sentence.into());
            })?;
            if let Some(_) = extracted_sentence {
                break;
            }
        }
        match &mut self.transaction {
            Some(transaction) => {
                let result = Ok(Some(transaction.process_response(
                    extracted_sentence.expect("Can't be None").as_slice(),
                )?));
                assert!(transaction.state() == TransactionState::Dead);
                self.transaction = None;
                result
            }
            // We don't expect data
            None => Err(Error::SpuriousSentence),
        }
    }

    fn update_time(&mut self, now: Instant) {
        self.now = now;
    }

    fn next_id(&mut self) -> usize {
        self.command_id = (self.command_id + 1) % 1000;
        self.command_id
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
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let _consort = Consort::new(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            &mut ringbuffer,
            Instant::now(),
        );
    }

    #[test]
    fn test_sending_command() {
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut consort = Consort::new(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            &mut ringbuffer,
            Instant::now(),
        );
        let mut mock_port = MockPort::default();
        consort
            .send_command(Command::Reset, &mut mock_port)
            .unwrap();
        assert_eq!(
            mock_port.sent_messages.borrow_mut().pop(),
            Some(b"$LNCCMD,RESET,001,RQA*01\r\n".as_slice().into())
        );
        let mut inputbuffer = ringbuffer::AllocRingBuffer::new(256);
        for c in b"$RQAACK,123456.001,LNC,001*4F\r\n" {
            inputbuffer.push(*c);
        }
        assert_matches!(consort.feed(&mut inputbuffer), Ok(Some(_)));
        assert_matches!(consort.transaction, None);
    }

    #[test]
    fn test_sending_spurious_command() {
        let mut ringbuffer = ringbuffer::AllocRingBuffer::new(256);
        let mut consort = Consort::new(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            &mut ringbuffer,
            Instant::now(),
        );

        let mut inputbuffer = ringbuffer::AllocRingBuffer::new(256);
        for c in b"$RQAACK,123456.001,LNC,001*4F\r\n" {
            inputbuffer.push(*c);
        }
        assert_matches!(consort.feed(&mut inputbuffer), Err(Error::SpuriousSentence));
    }

    #[test]
    fn test_sending_more_than_one_command() {
        todo!();
    }
}
