use std::time::Duration;

use uom::si::f64::{Force, Pressure};

use super::{AdcForceCalibration, AdcPressureCalibration, Ads1256Reading, ClkFreq, Timestamp};

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RawObservablesGroup1 {
    pub clkfreq: ClkFreq,
    pub uptime: Timestamp,
    pub thrust: Ads1256Reading,
    pub pressure: Ads1256Reading,
}

#[derive(Clone, PartialEq, Debug)]
pub struct RawObservablesGroup2 {
    pub state: u8,
    pub filename_or_error: Vec<u8>,
    pub anomalies: u32,
    pub vbb_voltage: u16,
    pub pyro_status: u8,
    pub records: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RawObservablesGroup {
    OG1(RawObservablesGroup1),
    OG2(RawObservablesGroup2),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ObservablesGroup1 {
    pub clkfreq: ClkFreq,
    pub uptime: Duration,
    pub thrust: Force,
    pub pressure: Pressure,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RecordingState {
    Unknown,
    Error(String),
    Pause,
    Recording(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum PyroStatus {
    Unknown,
    Open,
    Closed,
}

#[derive(Clone, Debug)]
pub struct ObservablesGroup2 {
    pub recording_state: RecordingState,
    pub anomalies: u32,
    pub records: u32,
    pub vbb_voltage: f32,
    pub pyro12_status: PyroStatus,
    pub pyro34_status: PyroStatus,
}

pub struct SystemDefinition {
    thrust_calibration: AdcForceCalibration,
    pressure_calibration: AdcPressureCalibration,
}

impl Default for SystemDefinition {
    fn default() -> Self {
        let thrust_calibration = AdcForceCalibration {
            m: 4.451e-5,
            c: -0.049,
        };
        let pressure_calibration = AdcPressureCalibration {
            m: 4.213e-5,
            c: -0.927,
        };

        Self {
            thrust_calibration,
            pressure_calibration,
        }
    }
}

impl SystemDefinition {
    pub fn transform_og1(&self, raw: &RawObservablesGroup1) -> ObservablesGroup1 {
        let uptime = raw.uptime.duration(&raw.clkfreq);
        let thrust = self.thrust_calibration.force(raw.thrust.clone());
        let pressure = self.pressure_calibration.pressure(raw.pressure.clone());
        ObservablesGroup1 {
            clkfreq: raw.clkfreq,
            uptime,
            thrust,
            pressure,
        }
    }

    pub fn transform_og2(&self, raw: &RawObservablesGroup2) -> ObservablesGroup2 {
        fn pyro_status_from_bitfield(value: u8) -> PyroStatus {
            match value {
                0 => PyroStatus::Unknown,
                2 => PyroStatus::Open,
                3 => PyroStatus::Closed,
                _ => unreachable!(),
            }
        }

        let anomalies = raw.anomalies;
        let vbb_voltage = raw.vbb_voltage as f32 * 0.00125;
        ObservablesGroup2 {
            recording_state: match raw.state {
                b'U' => RecordingState::Unknown,
                b'P' => RecordingState::Pause,
                b'E' => RecordingState::Error(
                    std::str::from_utf8(&raw.filename_or_error)
                        .unwrap()
                        .to_string(),
                ),
                b'R' => RecordingState::Recording(
                    std::str::from_utf8(&raw.filename_or_error)
                        .unwrap()
                        .to_string(),
                ),
                _ => unreachable!(),
            },
            anomalies,
            records: raw.records,
            vbb_voltage,
            pyro12_status: pyro_status_from_bitfield(raw.pyro_status & 0x03),
            pyro34_status: pyro_status_from_bitfield(raw.pyro_status >> 4 & 0x03),
        }
    }
}
