use std::{ops::Range, time::Duration};

use crate::rqparser::{
    ack_parser, nibble_to_hex, one_return_value_parser, two_return_values_parser,
    verify_nmea_format, NMEAFormatError,
};

#[derive(Debug, PartialEq)]
pub enum FormatErrorDetail {
    FormatError,
    NoChecksumError,
    ChecksumError,
    SentenceTooLongError,
    NoSentenceAvailable,
    TrailingCharacters,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    BufferLengthError,
    FormatError(FormatErrorDetail),
    ParseError,
    Nak,
    InvalidAssociation,
}

#[derive(Debug, PartialEq)]
pub struct RqTimestamp {
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub seconds: u8,
    pub fractional: Duration,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Node {
    RedQueen(u8),  // RQ<X>
    Farduino(u8),  // FD<X>
    LaunchControl, // LNC
}

#[derive(Debug, PartialEq)]
pub struct AckHeader {
    // The node sending this message
    pub source: Node,
    // The node the original command was issued from
    // and this ack is the destination for.
    pub recipient: Node,
    pub timestamp: RqTimestamp,
    pub id: usize,
}

/// All commands known to the RQ protocol
#[derive(Debug, PartialEq)]
pub enum Command {
    Reset,
    LaunchSecretPartial(u8),
    LaunchSecretFull(u8, u8),
    Ignition,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Response {
    Ack,
}

enum CommandProcessor {
    Ack,
    LaunchSecretPartial(u8),
    LaunchSecretFull(u8, u8),
}

impl Command {
    fn verb(&self) -> &'static [u8] {
        match self {
            Command::Reset => b"RESET",
            Command::LaunchSecretPartial(_) => b"SECRET_A",
            Command::LaunchSecretFull(_, _) => b"SECRET_AB",
            Command::Ignition => b"IGNITION",
        }
    }

    fn processor(&self) -> CommandProcessor {
        match self {
            Command::Reset => CommandProcessor::Ack,
            Command::LaunchSecretPartial(a) => CommandProcessor::LaunchSecretPartial(*a),
            Command::LaunchSecretFull(a, b) => CommandProcessor::LaunchSecretFull(*a, *b),
            Command::Ignition => CommandProcessor::Ack,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Acknowledgement {
    Ack(AckHeader),
    Nak(AckHeader),
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TransactionState {
    Alive,
    Dead,
}

#[derive(Debug)]
pub struct Transaction {
    // Us, that we send the message
    source: Node,
    // The destination of the message
    recipient: Node,
    id: usize,
    command: Command,
    state: TransactionState,
}

pub trait Serialize {
    fn serialize<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error>;
}

// Just translate the errors, we don't care about the
// NMEA sentence without checksum, as we are not using
// that.
impl From<NMEAFormatError<'_>> for Error {
    fn from(value: NMEAFormatError) -> Self {
        match value {
            NMEAFormatError::FormatError => Error::FormatError(FormatErrorDetail::FormatError),
            NMEAFormatError::NoChecksumError(_) => {
                Error::FormatError(FormatErrorDetail::NoChecksumError)
            }
            NMEAFormatError::ChecksumError => Error::FormatError(FormatErrorDetail::ChecksumError),
            NMEAFormatError::SentenceTooLongError => {
                Error::FormatError(FormatErrorDetail::SentenceTooLongError)
            }
            NMEAFormatError::NoSentenceAvailable => {
                Error::FormatError(FormatErrorDetail::NoSentenceAvailable)
            }
        }
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for Error {
    fn from(value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        Error::ParseError
    }
}

fn range_check(inner: &Range<usize>, outer: &Range<usize>) -> Result<(), Error> {
    // We encode a special-case here: if the inner range is right at the
    // end of the interval it is allowed to be empty. This facilitates
    // checking for the resulting range being still valid filling up
    // the buffer to the end.
    if inner.is_empty() && inner.end == outer.end {
        Ok(())
    } else {
        // Range is half-open right, so this should work
        if outer.contains(&inner.start) && outer.contains(&(inner.end - 1)) {
            Ok(())
        } else {
            Err(Error::BufferLengthError)
        }
    }
}

fn range_check_for_length(
    inner: &Range<usize>,
    outer: &Range<usize>,
    len: usize,
) -> Result<(), Error> {
    if inner.end - inner.start >= len {
        range_check(inner, outer)?;
        range_check(&(inner.start + len..inner.end), outer)
    } else {
        Err(Error::BufferLengthError)
    }
}

fn range_check_buffer_for_length(
    range: &Range<usize>,
    buffer: &[u8],
    len: usize,
) -> Result<(), Error> {
    range_check_for_length(range, &(0..buffer.len()), len)
}

impl Serialize for Node {
    fn serialize<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        range_check_buffer_for_length(&range, buffer, 3)?;
        match self {
            Node::RedQueen(n) => {
                buffer[range.clone()][0..2].copy_from_slice(b"RQ");
                buffer[range.clone()][2] = *n;
            }
            Node::Farduino(n) => {
                buffer[range.clone()][0..2].copy_from_slice(b"FD");
                buffer[range.clone()][2] = *n;
            }
            Node::LaunchControl => {
                buffer[range.clone()][0..3].copy_from_slice(b"LNC");
            }
        }
        Ok(range.start + 3..range.end)
    }
}

fn u8_parameter(buffer: &mut [u8], range: Range<usize>, param: u8) -> Result<Range<usize>, Error> {
    range_check_buffer_for_length(&range, buffer, 3)?;
    let data: [u8; 3] = [b',', nibble_to_hex(param >> 4), nibble_to_hex(param & 0xf)];
    buffer[range.clone()][0..3].copy_from_slice(&data);
    Ok(range.start + 3..range.end)
}

impl Serialize for Command {
    fn serialize<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        match self {
            Command::Reset => Ok(range),
            Command::LaunchSecretPartial(a) => u8_parameter(buffer, range, *a),
            Command::LaunchSecretFull(a, b) => {
                let range = u8_parameter(buffer, range, *a)?;
                u8_parameter(buffer, range, *b)
            }
            Command::Ignition => Ok(range),
        }
    }
}

fn append_bytes(buffer: &mut [u8], range: Range<usize>, s: &[u8]) -> Result<Range<usize>, Error> {
    range_check(&range, &(0..buffer.len()))?;
    range_check(&(range.start + s.len()..range.end), &(0..buffer.len()))?;
    for (i, c) in s.iter().enumerate() {
        buffer[range.start + i] = *c;
    }
    Ok(range.start + s.len()..range.end)
}

fn serialize_count(
    buffer: &mut [u8],
    range: Range<usize>,
    id: usize,
) -> Result<Range<usize>, Error> {
    range_check_buffer_for_length(&range, buffer, 3)?;
    let (a, b, c) = ((id / 100 % 10), (id / 10 % 10), id % 10);
    let data: [u8; 3] = [a as u8 + b'0', b as u8 + b'0', c as u8 + b'0'];
    buffer[range.clone()][0..3].copy_from_slice(&data);
    Ok(range.start + 3..range.end)
}

impl Serialize for Transaction {
    fn serialize<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        let range = self.source.serialize(buffer, range)?;
        let range = append_bytes(buffer, range, b"CMD,")?;
        let range = append_bytes(buffer, range, self.command.verb())?;
        let range = append_bytes(buffer, range, b",")?;
        let range = serialize_count(buffer, range, self.id)?;
        let range = append_bytes(buffer, range, b",")?;
        let range = self.recipient.serialize(buffer, range)?;
        Ok(self.command.serialize(buffer, range)?)
    }
}

impl Transaction {
    pub fn new(sender: Node, recipient: Node, id: usize, command: Command) -> Self {
        Self {
            source: sender,
            recipient,
            id,
            command,
            state: TransactionState::Alive,
        }
    }

    pub fn state(&self) -> TransactionState {
        self.state.clone()
    }

    pub fn process_response(&mut self, sentence: &[u8]) -> Result<Response, Error> {
        // Currently, all commands lead to the Transaction
        // being dead, so let's just hard-code this here
        self.state = TransactionState::Dead;
        let contents = verify_nmea_format(sentence)?;
        let (rest, response) = ack_parser(contents)?;
        match response {
            Acknowledgement::Ack(AckHeader {
                source,
                recipient,
                id,
                ..
            }) => {
                // source and recipient are crossed over
                if id == self.id && source == self.recipient && recipient == self.source {
                    let (trailing, response) = self.command.processor().process_response(rest)?;
                    if trailing.is_empty() {
                        Ok(response)
                    } else {
                        Err(Error::FormatError(FormatErrorDetail::TrailingCharacters))
                    }
                } else {
                    Err(Error::InvalidAssociation)
                }
            }
            Acknowledgement::Nak(_) => Err(Error::Nak),
        }
    }
}

impl CommandProcessor {
    fn process_response<'a>(&self, params: &'a [u8]) -> Result<(&'a [u8], Response), Error> {
        match self {
            CommandProcessor::LaunchSecretPartial(a) => {
                let (rest, param) = one_return_value_parser(params)?;
                if param == *a {
                    Ok((rest, Response::Ack))
                } else {
                    Err(Error::ParseError)
                }
            }
            CommandProcessor::LaunchSecretFull(a, b) => {
                let (rest, (param1, param2)) = two_return_values_parser(params)?;
                if param1 == *a && param2 == *b {
                    Ok((rest, Response::Ack))
                } else {
                    Err(Error::ParseError)
                }
            }
            CommandProcessor::Ack => Ok((params, Response::Ack)),
            _ => Err(Error::ParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rqparser::NMEAFormatter;
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_launch_secret_partial() {
        let command = Command::LaunchSecretPartial(0x3f);
        let sender = Node::LaunchControl;
        let recipient = Node::RedQueen(b'A');
        let id = 123;
        let mut t = Transaction::new(sender, recipient, id, command);

        let mut dest: [u8; 82] = [0; 82];
        let remaining = t.serialize(&mut dest, 0..82).unwrap();
        let mut formatter = NMEAFormatter::default();
        let _result = formatter
            .format_sentence(&dest[0..remaining.start])
            .unwrap();
        assert_eq!(
            formatter.buffer().unwrap(),
            b"$LNCCMD,SECRET_A,123,RQA,3F*04\r\n".as_slice()
        );
        assert_matches!(
            t.process_response(b"$RQAACK,123456.001,LNC,123,3F*17\r\n"),
            Ok(_),
        );
    }

    #[test]
    fn test_launch_secret_full() {
        let command = Command::LaunchSecretFull(0x3f, 0xab);
        let sender = Node::LaunchControl;
        let recipient = Node::RedQueen(b'A');
        let id = 123;
        let mut t = Transaction::new(sender, recipient, id, command);

        let mut dest: [u8; 82] = [0; 82];
        let remaining = t.serialize(&mut dest, 0..82).unwrap();
        let mut formatter = NMEAFormatter::default();
        let _result = formatter
            .format_sentence(&dest[0..remaining.start])
            .unwrap();
        assert_eq!(
            formatter.buffer().unwrap(),
            b"$LNCCMD,SECRET_AB,123,RQA,3F,AB*69\r\n".as_slice()
        );
        assert_matches!(
            t.process_response(b"$RQAACK,123456.001,LNC,123,3F,AB*38\r\n"),
            Ok(_),
        );
        assert_matches!(
            t.process_response(b"$RQAACK,123456.001,LNC,123,3F,ABfoo*5E\r\n"),
            Err(Error::FormatError(FormatErrorDetail::TrailingCharacters)),
        );
    }

    #[test]
    fn test_reset() {
        let command = Command::Reset;
        let sender = Node::LaunchControl;
        let recipient = Node::RedQueen(b'A');
        let id = 123;
        let mut t = Transaction::new(sender, recipient, id, command);
        assert_eq!(t.state(), TransactionState::Alive);
        let mut dest: [u8; 82] = [0; 82];
        let remaining = t.serialize(&mut dest, 0..82).unwrap();
        let mut formatter = NMEAFormatter::default();
        let _result = formatter
            .format_sentence(&dest[0..remaining.start])
            .unwrap();
        assert_eq!(
            formatter.buffer().unwrap(),
            b"$LNCCMD,RESET,123,RQA*00\r\n".as_slice()
        );
        assert_matches!(
            t.process_response(b"$RQAACK,123456.001,LNC,123*4E\r\n"),
            Ok(_),
        );
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_ignition() {
        let command = Command::Ignition;
        let sender = Node::LaunchControl;
        let recipient = Node::RedQueen(b'A');
        let id = 123;
        let mut t = Transaction::new(sender, recipient, id, command);

        let mut dest: [u8; 82] = [0; 82];
        let remaining = t.serialize(&mut dest, 0..82).unwrap();
        let mut formatter = NMEAFormatter::default();
        let _result = formatter
            .format_sentence(&dest[0..remaining.start])
            .unwrap();
        assert_eq!(
            formatter.buffer().unwrap(),
            b"$LNCCMD,IGNITION,123,RQA*40\r\n".as_slice()
        );
        assert_matches!(
            t.process_response(b"$RQAACK,123456.001,LNC,123*4E\r\n"),
            Ok(_),
        );
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_range_check() {
        assert_matches!(range_check(&(0..9), &(0..10)), Ok(_));
        assert_matches!(range_check(&(0..10), &(0..10)), Ok(_));
        assert_matches!(range_check(&(1..10), &(0..10)), Ok(_));
        assert_matches!(range_check(&(1..11), &(0..10)), Err(_));
        assert_matches!(range_check(&(0..10), &(1..10)), Err(_));
    }

    #[test]
    fn test_too_small_buffer_handling() {
        let sender = Node::LaunchControl;
        let mut dest: [u8; 10] = [0; 10];
        assert_matches!(sender.serialize(&mut dest, 0..10), Ok(_));
        assert_eq!(&dest[0..3], b"LNC");
        assert_matches!(sender.serialize(&mut dest, 1..10), Ok(_));
        assert_eq!(&dest[1..4], b"LNC");
        assert_matches!(sender.serialize(&mut dest, 7..10), Ok(_));
        assert_eq!(&dest[7..10], b"LNC");
        assert_matches!(sender.serialize(&mut dest, 0..2), Err(_));
    }
}
