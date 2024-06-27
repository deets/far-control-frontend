use super::{
    command_id_parser, hex_i32_parser, hex_u16_parser, hex_u32_parser, hex_u64_parser,
    hex_u8_parser, node_parser, string_parser,
};
use nom::{branch::alt, bytes::complete::tag, sequence::tuple, IResult};

use crate::{
    observables::{
        rqa::{RawObservablesGroup, RawObservablesGroup1, RawObservablesGroup2},
        Ads1256Reading, ClkFreq, Timestamp,
    },
    rqprotocol::Node,
};

fn obg1_parser(s: &[u8]) -> IResult<&[u8], (Node, usize, Node, RawObservablesGroup)> {
    // RQAOBG,123,LNC,1,0BEBC200,00000000AA894CC8,000669E2
    let (rest, (source, _, command_id, _, recipient, _, clkfreq, _, timestamp, _, adc0, _, adc1)) =
        tuple((
            node_parser,
            tag(b"OBG,"),
            command_id_parser,
            tag(b","),
            node_parser,
            tag(",1,"),
            hex_u32_parser,
            tag(","),
            hex_u64_parser,
            tag(","),
            hex_i32_parser,
            tag(","),
            hex_i32_parser,
        ))(s)?;
    Ok((
        rest,
        (
            source,
            command_id,
            recipient,
            RawObservablesGroup::OG1(RawObservablesGroup1 {
                clkfreq: ClkFreq(clkfreq),
                uptime: Timestamp(timestamp),
                thrust: Ads1256Reading(adc0),
                pressure: Ads1256Reading(adc1),
            }),
        ),
    ))
}

fn obg2_parser(s: &[u8]) -> IResult<&[u8], (Node, usize, Node, RawObservablesGroup)> {
    // RQAOBG,123,LNC,2,R,FOOBAR.TXT,000000FF,12345678,ABCD,22
    let (
        rest,
        (
            source,
            _,
            command_id,
            _,
            recipient,
            _,
            state,
            _,
            filename_or_error,
            _,
            anomalies,
            _,
            records,
            _,
            vbb_voltage,
            _,
            pyro_status,
        ),
    ) = tuple((
        node_parser,
        tag(b"OBG,"),
        command_id_parser,
        tag(b","),
        node_parser,
        tag(",2,"),
        alt((tag("E"), tag("P"), tag("U"), tag("R"))),
        tag(","),
        string_parser,
        tag(","),
        hex_u32_parser,
        tag(","),
        hex_u32_parser,
        tag(","),
        hex_u16_parser,
        tag(","),
        hex_u8_parser,
    ))(s)?;
    Ok((
        rest,
        (
            source,
            command_id,
            recipient,
            RawObservablesGroup::OG2(RawObservablesGroup2 {
                state: state[0],
                filename_or_error,
                anomalies,
                vbb_voltage,
                pyro_status,
                records,
            }),
        ),
    ))
}

pub fn obg_parser(s: &[u8]) -> IResult<&[u8], (Node, usize, Node, RawObservablesGroup)> {
    Ok(alt((obg1_parser, obg2_parser))(s)?)
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use crate::observables::rqa::RawObservablesGroup1;

    use super::*;

    #[test]
    fn test_feeding_full_sentence() {
        let sentence = b"$RQSTATE,013940.4184,DROGUE_OPEN*39\r\n";
        let mut parser = SentenceParser::new();
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
        let mut parser = SentenceParser::new();
        assert_eq!(
            Err(Error::OutputBufferOverflow),
            parser.feed(sentence, |_| {})
        );
    }

    #[test]
    fn test_output_buffer_overvflow_recovers() {
        let sentence = b"$RQSTATE,01234567890123456789012345678901234567890123456789012345678901234567890123456789013940.4184,DROGUE_OPEN*39\r\n";
        let mut parser = SentenceParser::new();

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
        let mut parser = SentenceParser::new();
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
        let mut parser = SentenceParser::new();
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
        let inner_sentence = b"RQEACK,123,LNC";
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
        let inner_sentence = b"RQENAK,123,LNC";
        assert_matches!(ack_parser(inner_sentence), Ok((_, Acknowledgement::Nak(_))));
    }

    #[test]
    fn test_return_argument_parsing() {
        assert_matches!(one_hex_return_value_parser(b",3F"), Ok((b"", 0x3f)));
        assert_matches!(two_return_values_parser(b",AB,CD"), Ok((b"", (0xab, 0xcd))));
    }

    #[test]
    fn test_command_parsing() {
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,RESET,40"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::Reset(AdcGain::Gain64),
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,IGNITION"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::Ignition,
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,UNLOCK_PYROS"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::UnlockPyros,
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,SECRET_A,AB"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::LaunchSecretPartial(0xab),
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,OBG,01"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::ObservableGroup(0x01),
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,SECRET_AB,AB,CD"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::LaunchSecretFull(0xab, 0xcd),
                    ..
                }
            ))
        );
        assert_matches!(
            command_parser(b"LNCCMD,123,RQA,PING"),
            Ok((
                b"",
                Transaction {
                    id: 123,
                    source: Node::LaunchControl,
                    recipient: Node::RedQueen(b'A'),
                    command: Command::Ping,
                    ..
                }
            ))
        );
    }

    #[test]
    fn test_actual_response() {
        assert_matches!(ack_parser(b"RQAACK,001,LNC,RESET"), Ok(_));
    }

    #[test]
    fn test_usize_parser() {
        assert_matches!(usize_parser(b"1"), Ok((b"", 1)));
    }

    #[test]
    fn test_obg1_parser() {
        assert_matches!(
            rqa_obg1_parser(b"RQAOBG,006,LNC,1,0BEBC200,000000003440E810,00069B00,FFFFFA7B"),
            Ok(_)
        );
        //b'OBG,003,LNC,1,0BEBC200,000000059681E328,00069BB7,FFFFFA79'

        assert_matches!(
            rqa_obg1_parser(b"RQAOBG,123,LNC,1,0BEBC200,00000000AA894CC8,FFFFFFFF,00000000"),
            Ok((
                b"",
                (
                    Node::RedQueen(b'A'),
                    123,
                    Node::LaunchControl,
                    RawObservablesGroup1 {
                        clkfreq: ClkFreq(0x0BEBC200),
                        uptime: Timestamp(0x00000000AA894CC8),
                        thrust: Ads1256Reading(-1),
                        pressure: Ads1256Reading(0),
                    }
                )
            ))
        );
    }

    #[test]
    fn test_string() {
        let (rest, contents) = string_parser(b"TEST.DAT").unwrap();
        assert_eq!(contents.as_slice(), b"TEST.DAT");
        assert_eq!(rest, b"");
        let (rest, contents) = string_parser(b"TEST.DAT,").unwrap();
        assert_eq!(contents.as_slice(), b"TEST.DAT");
        assert_eq!(rest, b",");
        let (rest, contents) = string_parser(b"TEST.DAT*").unwrap();
        assert_eq!(contents.as_slice(), b"TEST.DAT");
        assert_eq!(rest, b"*");
    }

    #[test]
    fn test_obg2_parser() {
        assert_matches!(
            rqa_obg2_parser(b"RQAOBG,010,LNC,2,R,RQADS002.TXT,00000064,00000579,007D,00"),
            Ok((
                b"",
                (
                    Node::RedQueen(b'A'),
                    10,
                    Node::LaunchControl,
                    RawObservablesGroup2 {
                        state: b'R',
                        anomalies: 100,
                        vbb_voltage: 125,
                        pyro_status: 0x00,
                        records: 1401,
                        ..
                    }
                )
            ))
        );
    }
}
