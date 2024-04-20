use std::{fmt::Display, ops::Range, time::Duration};

use crate::{
    observables::rqa::RawObservablesGroup,
    rqparser::{
        ack_parser, command_parser, nibble_to_hex, one_hex_return_value_parser,
        one_usize_return_value_parser, rqa_obg_parser, two_return_values_parser,
        verify_nmea_format, NMEAFormatError, NMEAFormatter, MAX_BUFFER_SIZE,
    },
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
    InvalidAssociation(Node, Node, usize, usize),
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
    pub id: usize,
}

/// All commands known to the RQ protocol
#[derive(Debug, PartialEq)]
pub enum Command {
    Reset,
    LaunchSecretPartial(u8),
    UnlockPyros,
    LaunchSecretFull(u8, u8),
    Ignition,
    Ping,
    ObservableGroup(usize),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Response {
    ResetAck,
    IgnitionAck,
    LaunchSecretFullAck,
    UnlockPyrosAck,
    LaunchSecretPartialAck,
    PingAck,
    ObservableGroup(RawObservablesGroup),
    ObservableGroupAck,
}

enum CommandProcessor {
    ResetAck,
    LaunchSecretPartial(u8),
    UnlockPyrosAck,
    LaunchSecretFull(u8, u8),
    IgnitionAck,
    PingAck,
    ObservableGroupAck(usize),
}

impl Command {
    fn verb(&self) -> &'static [u8] {
        match self {
            Command::Reset => b"RESET",
            Command::LaunchSecretPartial(_) => b"SECRET_A",
            Command::UnlockPyros => b"UNLOCK_PYROS",
            Command::LaunchSecretFull(_, _) => b"SECRET_AB",
            Command::Ignition => b"IGNITION",
            Command::Ping => b"PING",
            Command::ObservableGroup(_) => b"OBG",
        }
    }

    fn processor(&self) -> CommandProcessor {
        match self {
            Command::Reset => CommandProcessor::ResetAck,
            Command::LaunchSecretPartial(a) => CommandProcessor::LaunchSecretPartial(*a),
            Command::UnlockPyros => CommandProcessor::UnlockPyrosAck,
            Command::LaunchSecretFull(a, b) => CommandProcessor::LaunchSecretFull(*a, *b),
            Command::Ignition => CommandProcessor::IgnitionAck,
            Command::Ping => CommandProcessor::PingAck,
            Command::ObservableGroup(g) => CommandProcessor::ObservableGroupAck(*g),
        }
    }
    fn process_response(
        &self,
        transaction: &Transaction,
        contents: &[u8],
    ) -> Result<(TransactionState, Response), Error> {
        match self {
            Command::ObservableGroup(_group) => self.process_obg_response(transaction, contents),
            _ => self.process_immediate_response(transaction, contents),
        }
    }

    fn process_obg_response(
        &self,
        transaction: &Transaction,
        contents: &[u8],
    ) -> Result<(TransactionState, Response), Error> {
        match rqa_obg_parser(contents) {
            Ok((_rest, (_source, _command_id, _sender, raw))) => {
                // TODO: a lot of checking!
                Ok((TransactionState::Alive, Response::ObservableGroup(raw)))
            }
            Err(_) => self.process_immediate_response(transaction, contents),
        }
    }

    fn process_immediate_response(
        &self,
        transaction: &Transaction,
        contents: &[u8],
    ) -> Result<(TransactionState, Response), Error> {
        let (rest, response) = ack_parser(contents)?;
        match response {
            Acknowledgement::Ack(AckHeader {
                source,
                recipient,
                id,
                ..
            }) => {
                // source and recipient are crossed over
                if id == transaction.id
                    && source == transaction.recipient
                    && recipient == transaction.source
                {
                    let (trailing, response) = self.processor().process_response(rest)?;
                    if trailing.is_empty() {
                        Ok((TransactionState::Dead, response))
                    } else {
                        Err(Error::FormatError(FormatErrorDetail::TrailingCharacters))
                    }
                } else {
                    Err(Error::InvalidAssociation(
                        source,
                        recipient,
                        id,
                        transaction.id,
                    ))
                }
            }
            Acknowledgement::Nak(_) => Err(Error::Nak),
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
    pub source: Node,
    // The destination of the message
    pub recipient: Node,
    pub id: usize,
    pub command: Command,
    state: TransactionState,
}

pub trait Marshal {
    fn to_command<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error>;

    fn to_acknowledgement<'a>(
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
    fn from(_value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        Error::ParseError
    }
}

impl std::error::Error for Error {}

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

impl Marshal for Node {
    fn to_command<'a>(
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

    fn to_acknowledgement<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        self.to_command(buffer, range)
    }
}

fn u8_parameter(buffer: &mut [u8], range: Range<usize>, param: u8) -> Result<Range<usize>, Error> {
    range_check_buffer_for_length(&range, buffer, 3)?;
    let data: [u8; 3] = [b',', nibble_to_hex(param >> 4), nibble_to_hex(param & 0xf)];
    buffer[range.clone()][0..3].copy_from_slice(&data);
    Ok(range.start + 3..range.end)
}

fn number_length(value: usize) -> usize {
    match value {
        0 => 1,
        _ => {
            let mut res = 0;
            let mut value = value;
            while value > 0 {
                res += 1;
                value /= 10;
            }
            res
        }
    }
}

fn usize_parameter(
    buffer: &mut [u8],
    range: Range<usize>,
    param: usize,
) -> Result<Range<usize>, Error> {
    let len = number_length(param);
    range_check_buffer_for_length(&range, buffer, len)?;
    let mut data: [u8; 22] = [b','; 22]; // ceil(log10(2**64)) + 1
    let mut value = param;
    for i in 0..len {
        data[len - i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    buffer[range.clone()][0..len + 1].copy_from_slice(&data[0..len + 1]);
    Ok(range.start + len + 1..range.end)
}

impl Marshal for Command {
    fn to_command<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        match self {
            Command::Reset => Ok(range),
            Command::LaunchSecretPartial(a) => u8_parameter(buffer, range, *a),
            Command::UnlockPyros => Ok(range),
            Command::LaunchSecretFull(a, b) => {
                let range = u8_parameter(buffer, range, *a)?;
                u8_parameter(buffer, range, *b)
            }
            Command::Ignition => Ok(range),
            Command::Ping => Ok(range),
            Command::ObservableGroup(group) => usize_parameter(buffer, range, *group),
        }
    }

    fn to_acknowledgement<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        self.to_command(buffer, range)
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

impl Marshal for Transaction {
    fn to_command<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        let range = self.source.to_command(buffer, range)?;
        let range = append_bytes(buffer, range, b"CMD,")?;
        let range = serialize_count(buffer, range, self.id)?;
        let range = append_bytes(buffer, range, b",")?;
        let range = self.recipient.to_command(buffer, range)?;
        let range = append_bytes(buffer, range, b",")?;
        let range = append_bytes(buffer, range, self.command.verb())?;
        Ok(self.command.to_command(buffer, range)?)
    }

    fn to_acknowledgement<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        // b"$RQAACK,123456.001,LNC,123,3F*17\r\n"
        let range = self.recipient.to_command(buffer, range)?;
        let range = append_bytes(buffer, range, b"ACK,")?;
        let range = serialize_count(buffer, range, self.id)?;
        let range = append_bytes(buffer, range, b",")?;
        let range = self.source.to_command(buffer, range)?;
        let range = self.command.to_acknowledgement(buffer, range)?;
        Ok(range)
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

    pub fn from_sentence(sentence: &[u8]) -> Result<Self, Error> {
        let (_rest, transaction) = command_parser(sentence)?;
        Ok(transaction)
    }

    pub fn state(&self) -> TransactionState {
        self.state.clone()
    }

    pub fn process_response(&mut self, sentence: &[u8]) -> Result<Response, Error> {
        let contents = verify_nmea_format(sentence)?;
        // Currently, all commands lead to the Transaction
        // being dead, so let's just hard-code this here
        let (state, response) = self.command.process_response(self, contents)?;
        self.state = state;
        Ok(response)
    }

    pub fn commandeer<'a>(&self, dest: &'a mut [u8; MAX_BUFFER_SIZE]) -> Result<&'a [u8], Error> {
        let response = self.to_command(dest, 0..MAX_BUFFER_SIZE).unwrap();
        let mut formatter = NMEAFormatter::default();
        formatter.format_sentence(&dest[0..response.start])?;
        let res = formatter.buffer()?;
        let len = res.len();
        dest[0..len].copy_from_slice(res);
        Ok(&dest[0..len])
    }

    pub fn acknowledge<'a>(&self, dest: &'a mut [u8; MAX_BUFFER_SIZE]) -> Result<&'a [u8], Error> {
        let response = self.to_acknowledgement(dest, 0..MAX_BUFFER_SIZE).unwrap();
        let mut formatter = NMEAFormatter::default();
        formatter.format_sentence(&dest[0..response.start])?;
        let res = formatter.buffer()?;
        let len = res.len();
        dest[0..len].copy_from_slice(res);
        Ok(&dest[0..len])
    }
}

impl CommandProcessor {
    fn process_response<'a>(&self, params: &'a [u8]) -> Result<(&'a [u8], Response), Error> {
        match self {
            CommandProcessor::LaunchSecretPartial(a) => {
                let (rest, param) = one_hex_return_value_parser(params)?;
                if param == *a {
                    Ok((rest, Response::LaunchSecretPartialAck))
                } else {
                    Err(Error::ParseError)
                }
            }
            CommandProcessor::UnlockPyrosAck => Ok((params, Response::UnlockPyrosAck)),
            CommandProcessor::LaunchSecretFull(a, b) => {
                let (rest, (param1, param2)) = two_return_values_parser(params)?;
                if param1 == *a && param2 == *b {
                    Ok((rest, Response::LaunchSecretFullAck))
                } else {
                    Err(Error::ParseError)
                }
            }
            CommandProcessor::ResetAck => Ok((params, Response::ResetAck)),
            CommandProcessor::IgnitionAck => Ok((params, Response::IgnitionAck)),
            CommandProcessor::PingAck => Ok((params, Response::PingAck)),
            CommandProcessor::ObservableGroupAck(g) => {
                let (rest, param) = one_usize_return_value_parser(params)?;
                if param as usize == *g {
                    Ok((rest, Response::ObservableGroupAck))
                } else {
                    Err(Error::ParseError)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rqparser::MAX_BUFFER_SIZE;
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_launch_secret_partial() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,SECRET_A,3F").unwrap();

        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,SECRET_A,3F*04\r\n".as_slice());
        assert_eq!(
            t.acknowledge(&mut dest).unwrap(),
            b"$RQAACK,123,LNC,3F*23\r\n".as_slice()
        );
        assert_matches!(t.process_response(b"$RQAACK,123,LNC,3F*23\r\n"), Ok(_),);
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_launch_secret_full() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,SECRET_AB,3F,AB").unwrap();

        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,SECRET_AB,3F,AB*69\r\n".as_slice());

        assert_eq!(
            t.acknowledge(&mut dest).unwrap(),
            b"$RQAACK,123,LNC,3F,AB*0C\r\n".as_slice()
        );

        assert_matches!(t.process_response(b"$RQAACK,123,LNC,3F,AB*0C\r\n"), Ok(_),);
        assert_matches!(
            t.process_response(b"$RQAACK,123,LNC,3F,ABfoo*6A\r\n"),
            Err(Error::FormatError(FormatErrorDetail::TrailingCharacters)),
        );
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_reset() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,RESET").unwrap();
        assert_eq!(t.state(), TransactionState::Alive);
        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,RESET*00\r\n".as_slice());
        assert_eq!(
            t.acknowledge(&mut dest).unwrap(),
            b"$RQAACK,123,LNC*7A\r\n".as_slice()
        );

        assert_matches!(t.process_response(b"$RQAACK,123,LNC*7A\r\n"), Ok(_),);
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_ignition() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,IGNITION").unwrap();
        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,IGNITION*40\r\n".as_slice());
        assert_matches!(t.process_response(b"$RQAACK,123,LNC*7A\r\n"), Ok(_),);
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_observable_group_immediate_ack() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,OBG,1").unwrap();
        if let Command::ObservableGroup(group_id) = t.command {
            assert_eq!(group_id, 1);
        }
        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,OBG,1*02\r\n".as_slice());
        assert_eq!(
            t.acknowledge(&mut dest).unwrap(),
            b"$RQAACK,123,LNC,1*67\r\n".as_slice()
        );
        assert_matches!(t.process_response(b"$RQAACK,123,LNC,1*67\r\n"), Ok(_),);
        assert_eq!(t.state(), TransactionState::Dead);
    }

    #[test]
    fn test_observable_group_with_group_result_and_then_ack() {
        let mut t = Transaction::from_sentence(b"LNCCMD,123,RQA,OBG,1").unwrap();
        if let Command::ObservableGroup(group_id) = t.command {
            assert_eq!(group_id, 1);
        }
        let mut dest: [u8; MAX_BUFFER_SIZE] = [0; MAX_BUFFER_SIZE];
        let result = t.commandeer(&mut dest).unwrap();
        assert_eq!(result, b"$LNCCMD,123,RQA,OBG,1*02\r\n".as_slice());
        //
        assert_eq!(
            t.acknowledge(&mut dest).unwrap(),
            b"$RQAACK,123,LNC,1*67\r\n".as_slice()
        );
        assert_matches!(
            t.process_response(
                b"$RQAOBG,123,LNC,1,0BEBC200,00000000AA894CC8,000669E2,00000001*12\r\n"
            ),
            Ok(Response::ObservableGroup(..)),
        );
        assert_eq!(t.state(), TransactionState::Alive);
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
        assert_matches!(sender.to_command(&mut dest, 0..10), Ok(_));
        assert_eq!(&dest[0..3], b"LNC");
        assert_matches!(sender.to_command(&mut dest, 1..10), Ok(_));
        assert_eq!(&dest[1..4], b"LNC");
        assert_matches!(sender.to_command(&mut dest, 7..10), Ok(_));
        assert_eq!(&dest[7..10], b"LNC");
        assert_matches!(sender.to_command(&mut dest, 0..2), Err(_));
    }

    #[test]
    fn test_number_length() {
        assert_eq!(number_length(0), 1);
        assert_eq!(number_length(7), 1);
        assert_eq!(number_length(10), 2);
        assert_eq!(number_length(123456), 6);
    }

    #[test]
    fn test_usize_parameter() {
        let mut data = [0; 20];
        let len = data.len();
        let r = usize_parameter(&mut data, 0..len, 10);
        assert_eq!(r, Ok(3..20));
        assert_eq!(b",10", &data[0..3]);
        let r = usize_parameter(&mut data, 0..len, 123);
        assert_eq!(r, Ok(4..20));
        assert_eq!(b",123", &data[0..4]);
        let r = usize_parameter(&mut data, 0..len, 0);
        assert_eq!(r, Ok(2..20));
        assert_eq!(b",0", &data[0..2]);
    }
}
