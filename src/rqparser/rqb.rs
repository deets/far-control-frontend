use super::{
    command_id_parser, hex_i32_parser, hex_u16_parser, hex_u32_parser, hex_u64_parser,
    hex_u8_parser, node_parser, string_parser,
};
use nom::{branch::alt, bytes::complete::tag, sequence::tuple, IResult};

use crate::{
    observables::{
        rqb::{RawObservablesGroup, RawObservablesGroup1, RawObservablesGroup2},
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
    // RQAOBG,123,LNC,2,ABCD,22
    let (rest, (source, _, command_id, _, recipient, _, vbb_voltage, _, pyro_status)) = tuple((
        node_parser,
        tag(b"OBG,"),
        command_id_parser,
        tag(b","),
        node_parser,
        tag(",2,"),
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
                vbb_voltage,
                pyro_status,
            }),
        ),
    ))
}

pub fn obg_parser(s: &[u8]) -> IResult<&[u8], (Node, usize, Node, RawObservablesGroup)> {
    Ok(alt((obg1_parser, obg2_parser))(s)?)
}
