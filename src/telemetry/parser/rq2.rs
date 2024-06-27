use nom::{bytes::complete::take, sequence::tuple, IResult};

use crate::rqprotocol::Node;

const DEFAULT_ACC_RANGE: BMI088AccRange = BMI088AccRange::AccRange24g;
const DEFAULT_GYR_RANGE: BMI088GyrRange = BMI088GyrRange::GyrRange2000s;

#[derive(Debug, Clone, PartialEq)]
enum PacketType {
    StatePacket = 0,
    ImuSetAPacket = 1,
    ImuSetBPacket = 2,
}

#[derive(Debug, Clone)]
pub struct Preamble {
    seq: isize,
    packet_type: PacketType,
    timestamp: u32,
}

enum BMI088AccRange {
    AccRange3g,
    AccRange6g,
    AccRange12g,
    AccRange24g,
}

enum BMI088GyrRange {
    GyrRange2000s,
    GyrRange1000s,
    GyrRange500s,
    GyrRange250s,
    GyrRange125s,
}

#[derive(Debug, Clone)]
pub struct IMUReading {
    pub acc_x: f32,
    pub acc_y: f32,
    pub acc_z: f32,
    pub gyr_x: f32,
    pub gyr_y: f32,
    pub gyr_z: f32,
}

#[derive(Debug, Clone)]
pub struct MagReading {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone)]
pub struct IMUPacket {
    pub imu: IMUReading,
    pub mag: MagReading,
    pub pressure: f32,
    pub temperature: f32,
}

// This needs to be in sync with
// ignition-sm.h!
#[derive(Debug, Clone)]
pub enum IgnitionSMState {
    Reset,
    SecretA,
    PyrosUnlocked,
    SecretAB,
    Ignition,
    RadioSilence,
}

#[derive(Debug, Clone)]
pub enum TelemetryData {
    Ignition(IgnitionSMState),
    IMU(IMUPacket),
}

#[derive(Debug, Clone)]
pub struct TelemetryPacket {
    pub node: Node,
    pub preamble: Preamble,
    pub data: TelemetryData,
}

fn ignition_state_parser(s: &[u8]) -> IResult<&[u8], IgnitionSMState> {
    let (rest, c) = take(1 as usize)(s)?;
    let state = match c[0] {
        0 => IgnitionSMState::Reset,
        1 => IgnitionSMState::SecretA,
        2 => IgnitionSMState::PyrosUnlocked,
        3 => IgnitionSMState::SecretAB,
        4 => IgnitionSMState::Ignition,
        5 => IgnitionSMState::RadioSilence,
        _ => unreachable!(),
    };
    Ok((rest, state))
}

fn sequence_parser(s: &[u8]) -> IResult<&[u8], isize> {
    let (rest, c) = take(1 as usize)(s)?;
    Ok((rest, c[0] as isize))
}

fn packet_type_parser(s: &[u8]) -> IResult<&[u8], PacketType> {
    let (rest, c) = take(1 as usize)(s)?;
    let res = match c[0] {
        0 => PacketType::StatePacket,
        1 => PacketType::ImuSetAPacket,
        2 => PacketType::ImuSetBPacket,
        _ => panic!("Wrong packet type"),
    };
    Ok((rest, res))
}

fn u32_parser(s: &[u8]) -> IResult<&[u8], u32> {
    let (rest, prefix) = take(4 as usize)(s)?;
    let mut res: u32 = 0;
    for i in 0..4 {
        res |= (prefix[i] as u32) << (i * 8);
    }
    Ok((rest, res))
}

fn i16_parser(s: &[u8]) -> IResult<&[u8], i16> {
    let (rest, prefix) = take(2 as usize)(s)?;
    let mut res: u16 = 0;
    for i in 0..2 {
        res |= (prefix[i] as u16) << (i * 8);
    }
    Ok((rest, res as i16))
}

fn bmi088_parser(
    acc_range: BMI088AccRange,
    gyr_range: BMI088GyrRange,
    s: &[u8],
) -> IResult<&[u8], IMUReading> {
    let (rest, (ax, ay, az, gx, gy, gz)) = tuple((
        i16_parser, i16_parser, i16_parser, i16_parser, i16_parser, i16_parser,
    ))(s)?;
    let af = match acc_range {
        BMI088AccRange::AccRange3g => 2.0_f32.powf(1.0 + 1.0) * 1.5 / 32768.0,
        BMI088AccRange::AccRange6g => 2.0_f32.powf(2.0 + 1.0) * 1.5 / 32768.0,
        BMI088AccRange::AccRange12g => 2.0_f32.powf(3.0 + 1.0) * 1.5 / 32768.0,
        BMI088AccRange::AccRange24g => 2.0_f32.powf(4.0 + 1.0) * 1.5 / 32768.0,
    };
    let gf = match gyr_range {
        BMI088GyrRange::GyrRange2000s => 1.0 / 32768.0 * 2000.0,
        BMI088GyrRange::GyrRange1000s => 1.0 / 32768.0 * 1000.0,
        BMI088GyrRange::GyrRange500s => 1.0 / 32768.0 * 500.0,
        BMI088GyrRange::GyrRange250s => 1.0 / 32768.0 * 250.0,
        BMI088GyrRange::GyrRange125s => 1.0 / 32768.0 * 125.0,
    };
    Ok((
        rest,
        IMUReading {
            acc_x: ax as f32 * af,
            acc_y: ay as f32 * af,
            acc_z: az as f32 * af,
            gyr_x: gx as f32 * gf,
            gyr_y: gy as f32 * gf,
            gyr_z: gz as f32 * gf,
        },
    ))
}

fn mag_parser(s: &[u8]) -> IResult<&[u8], MagReading> {
    let (rest, (x, y, z)) = tuple((i16_parser, i16_parser, i16_parser))(s)?;
    Ok((
        rest,
        MagReading {
            x: x as f32,
            y: y as f32,
            z: z as f32,
        },
    ))
}

fn f32_parser(s: &[u8]) -> IResult<&[u8], f32> {
    let (rest, prefix) = take(4 as usize)(s)?;
    Ok((
        rest,
        f32::from_le_bytes(prefix.try_into().expect("Parser failed")),
    ))
}

fn preamble_parser(s: &[u8]) -> IResult<&[u8], Preamble> {
    let (rest, (seq, packet_type, timestamp)) =
        tuple((sequence_parser, packet_type_parser, u32_parser))(s)?;
    Ok((
        rest,
        Preamble {
            seq,
            packet_type,
            timestamp,
        },
    ))
}

fn imu_packet_parser(
    acc_range: BMI088AccRange,
    gyr_range: BMI088GyrRange,
    s: &[u8],
) -> IResult<&[u8], IMUPacket> {
    let (rest, imu) = bmi088_parser(acc_range, gyr_range, s)?;
    let (rest, mag) = mag_parser(rest)?;
    let (rest, (pressure, temperature)) = tuple((f32_parser, f32_parser))(rest)?;
    Ok((
        rest,
        IMUPacket {
            imu,
            mag,
            pressure,
            temperature,
        },
    ))
}

pub fn packet_parser(node: Node, s: &[u8]) -> IResult<&[u8], TelemetryPacket> {
    let (rest, preamble) = preamble_parser(s)?;
    let (rest, data) = match preamble.packet_type {
        PacketType::StatePacket => {
            let (rest, state) = ignition_state_parser(rest)?;
            (rest, TelemetryData::Ignition(state))
        }
        PacketType::ImuSetAPacket => {
            let (rest, packet) = imu_packet_parser(DEFAULT_ACC_RANGE, DEFAULT_GYR_RANGE, rest)?;
            (rest, TelemetryData::IMU(packet))
        }
        PacketType::ImuSetBPacket => {
            let (rest, packet) = imu_packet_parser(DEFAULT_ACC_RANGE, DEFAULT_GYR_RANGE, rest)?;
            (rest, TelemetryData::IMU(packet))
        }
    };
    Ok((
        rest,
        TelemetryPacket {
            data,
            preamble,
            node,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches::assert_matches;

    #[test]
    fn test_packet_type_parsing() {
        let sentence = b"\x02";
        let (_rest, packet_type) = packet_type_parser(sentence).unwrap();
        assert_eq!(packet_type, PacketType::ImuSetBPacket);
    }

    #[test]
    fn test_seq_parsing() {
        let sentence = b"\x02";
        let (_rest, seq) = sequence_parser(sentence).unwrap();
        assert_eq!(seq, 2);
    }

    #[test]
    fn test_number_parsing() {
        let (_rest, num) = i16_parser(b"\xff\xff").unwrap();
        assert_eq!(num, -1);
        let (_rest, num) = f32_parser(b"\x88\xf4\xab?").unwrap();
        assert_eq!(num, 1.3434);
    }

    #[test]
    fn test_preamble_parsing() {
        let sentence = b"\x01\x02\x0a\x0b\x0c\x0d";
        let (_rest, preamble) = preamble_parser(sentence).unwrap();
        assert_matches!(
            preamble,
            Preamble {
                seq: 1,
                packet_type: PacketType::ImuSetBPacket,
                timestamp: 0x0d0c0b0a,
            }
        );
    }

    #[test]
    fn test_imu_packet_parsing() {
        let sentence = b"\x00\x02\xfe\xb7\xdd\x81\xfd\xff\n\x00S\x05\x00\x00\xf9\xff\xfd\xff\x9b\x02^\xf7K\xf7\x8f\x8b{D\x00\x00\x00\x00";
        let (_rest, packet) = packet_parser(Node::RedQueen(b'B'), sentence).unwrap();
        assert_matches!(
            packet,
            TelemetryPacket {
                data: TelemetryData::IMU(..),
                ..
            }
        );
    }
    #[test]
    fn test_ignition_state_packet_parsing() {
        let sentence = b"A\x00~\xdcvV\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let (_rest, packet) = packet_parser(Node::RedQueen(b'B'), sentence).unwrap();
        assert_matches!(
            packet,
            TelemetryPacket {
                data: TelemetryData::Ignition(IgnitionSMState::Reset),
                ..
            }
        );
    }
}
