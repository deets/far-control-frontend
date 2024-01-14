use std::{ops::Range, time::Duration};

use crate::rqparser::nibble_to_hex;

#[derive(Debug)]
enum Error {
    BufferLengthError,
}

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

#[derive(Debug, PartialEq)]
pub struct Response {
    pub source: Node,
    pub sender: Node,
    pub timestamp: RqTimestamp,
    pub id: usize,
}

/// All commands known to the RQ protocol
pub enum Command {
    Reset,
    LaunchSecretPartial(u8),
    LaunchSecretFull(u8, u8),
    Ignition,
}

pub enum CommandAcknowledgement {
    Ack,
    LaunchSecretPartial(u8),
    LaunchSecretFull(u8, u8),
    Ignition,
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

    fn ack(&self) -> CommandAcknowledgement {
        match self {
            Command::Reset => CommandAcknowledgement::Ack,
            Command::LaunchSecretPartial(a) => CommandAcknowledgement::LaunchSecretPartial(*a),
            Command::LaunchSecretFull(a, b) => CommandAcknowledgement::LaunchSecretFull(*a, *b),
            Command::Ignition => CommandAcknowledgement::Ignition,
        }
    }
}

struct Transaction {
    sender: Node,
    recipient: Node,
    id: usize,
    command: Command,
}

trait Serialize {
    fn serialize<'a>(
        &self,
        buffer: &'a mut [u8],
        range: Range<usize>,
    ) -> Result<Range<usize>, Error>;
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
            Command::Ignition => Ok(0..10),
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
        let range = self.sender.serialize(buffer, range)?;
        let range = append_bytes(buffer, range, b"CMD,")?;
        let range = append_bytes(buffer, range, self.command.verb())?;
        let range = append_bytes(buffer, range, b",")?;
        let range = serialize_count(buffer, range, self.id)?;
        let range = append_bytes(buffer, range, b",")?;
        let range = self.recipient.serialize(buffer, range)?;
        Ok(self.command.serialize(buffer, range)?)
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
        let t = Transaction {
            sender,
            recipient,
            id,
            command,
        };

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
    }

    #[test]
    fn test_launch_secret_full() {
        let command = Command::LaunchSecretFull(0x3f, 0xab);
        let sender = Node::LaunchControl;
        let recipient = Node::RedQueen(b'A');
        let id = 123;
        let t = Transaction {
            sender,
            recipient,
            id,
            command,
        };

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
