use std::time::Duration;

use crate::rqparser::nibble_to_hex;

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

/// All commands known to the RQ protocol
pub enum Command {
    Reset,
    LaunchSecretPartial(u8),
    LaunchSecretFull(u8, u8),
    Ignition,
}

pub enum Ack {
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

    fn ack(&self) -> Ack {
        match self {
            Command::Reset => Ack::Ack,
            Command::LaunchSecretPartial(a) => Ack::LaunchSecretPartial(*a),
            Command::LaunchSecretFull(a, b) => Ack::LaunchSecretFull(*a, *b),
            Command::Ignition => Ack::Ignition,
        }
    }
}

#[derive(Debug)]
enum Error {
    BufferLengthError,
}

struct Transaction {
    sender: Node,
    recipient: Node,
    id: usize,
    command: Command,
}

trait Serialize {
    fn serialize<'a>(&self, buffer: &'a mut [u8]) -> Result<usize, Error>;
}

impl Serialize for Node {
    fn serialize<'a>(&self, buffer: &'a mut [u8]) -> Result<usize, Error> {
        match buffer.len() {
            d if d < 3 => Err(Error::BufferLengthError),
            _ => {
                match self {
                    Node::RedQueen(n) => {
                        buffer[0..2].copy_from_slice(b"RQ");
                        buffer[2] = *n;
                    }
                    Node::Farduino(n) => {
                        buffer[0..2].copy_from_slice(b"FD");
                        buffer[2] = *n;
                    }
                    Node::LaunchControl => {
                        buffer[0..3].copy_from_slice(b"LNC");
                    }
                }
                Ok(3)
            }
        }
    }
}

impl Transaction {
    fn serialize_count(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let (a, b, c) = ((self.id / 100 % 10), (self.id / 10 % 10), self.id % 10);
        buffer[0] = a as u8 + b'0';
        buffer[1] = b as u8 + b'0';
        buffer[2] = c as u8 + b'0';
        Ok(3)
    }
}

fn u8_parameter(buffer: &mut [u8], param: u8) -> Result<usize, Error> {
    match buffer.len() {
        d if d < 3 => Err(Error::BufferLengthError),
        _ => {
            buffer[0] = b',';
            buffer[1] = nibble_to_hex(param >> 4);
            buffer[2] = nibble_to_hex(param & 0xf);
            Ok(3)
        }
    }
}
impl Serialize for Command {
    fn serialize<'a>(&self, buffer: &'a mut [u8]) -> Result<usize, Error> {
        match self {
            Command::Reset => Ok(0),
            Command::LaunchSecretPartial(a) => u8_parameter(buffer, *a),
            Command::LaunchSecretFull(a, b) => {
                if buffer.len() >= 6 {
                    u8_parameter(buffer, *a)?;
                    u8_parameter(&mut buffer[3..6], *b)?;
                    Ok(6)
                } else {
                    Err(Error::BufferLengthError)
                }
            }
            Command::Ignition => Ok(0),
        }
    }
}
impl Serialize for Transaction {
    fn serialize<'a>(&self, buffer: &'a mut [u8]) -> Result<usize, Error> {
        let mut consumed = 0;
        let total = buffer.len();
        consumed += self.sender.serialize(buffer)?;
        buffer[consumed..consumed + 4].copy_from_slice(b"CMD,");
        consumed += 4;
        let verb = self.command.verb();
        buffer[consumed..consumed + verb.len()].copy_from_slice(verb);
        consumed += verb.len();
        buffer[consumed..consumed + 1].copy_from_slice(b",");
        consumed += 1;
        consumed += self.serialize_count(&mut buffer[consumed..consumed + 3])?;
        buffer[consumed..consumed + 1].copy_from_slice(b",");
        consumed += 1;
        consumed += self
            .recipient
            .serialize(&mut buffer[consumed..consumed + 3])?;
        consumed += self.command.serialize(&mut buffer[consumed..total])?;
        Ok(consumed)
    }
}

#[cfg(test)]
mod tests {
    use crate::rqparser::NMEAFormatter;

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
        let length = t.serialize(&mut dest).unwrap();
        let mut formatter = NMEAFormatter::default();
        let _result = formatter.format_sentence(&dest[0..length]).unwrap();
        assert_eq!(
            formatter.buffer().unwrap(),
            b"$LNCCMD,SECRET_A,123,RQA,3F*04\r\n".as_slice()
        );
    }
}
