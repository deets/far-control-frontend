use std::time::Duration;
use uom::si::f64::*;
use uom::si::mass::gram;

// Raw wire-values
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ClkFreq(pub u32);

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Timestamp(pub u64);

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Ads1256Reading(pub i32);

struct AdcWeightCalibration {
    m: f64,
    c: f64,
}

pub mod rqa {
    use std::time::Duration;

    use uom::si::f64::Mass;

    use super::{AdcWeightCalibration, Ads1256Reading, ClkFreq, Timestamp};

    #[derive(Copy, Clone, PartialEq, Debug)]
    pub struct RawObservablesGroup1 {
        pub clkfreq: ClkFreq,
        pub uptime: Timestamp,
        pub thrust: Ads1256Reading,
    }

    #[derive(Clone, PartialEq, Debug)]
    pub struct RawObservablesGroup2 {
        pub state: u8,
        pub filename_or_error: Vec<u8>,
        pub anomalies: u32,
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
        pub thrust: Mass,
    }

    #[derive(Clone, PartialEq, Debug)]
    pub enum RecordingState {
        Unknown,
        Error(String),
        Pause,
        Recording(String),
    }

    pub struct ObservablesGroup2 {
        pub recording_state: RecordingState,
        pub anomalies: u32,
    }

    pub struct SystemDefinition {
        thrust_calibration: AdcWeightCalibration,
    }

    impl Default for SystemDefinition {
        fn default() -> Self {
            let (m, c) = (127539.14190327494, -6423.647555776099);
            let calibration = AdcWeightCalibration { m, c };

            Self {
                thrust_calibration: calibration,
            }
        }
    }

    impl SystemDefinition {
        pub fn transform_og1(&self, raw: &RawObservablesGroup1) -> ObservablesGroup1 {
            let uptime = raw.uptime.duration(&raw.clkfreq);
            let thrust = self.thrust_calibration.weight(raw.thrust.clone());
            ObservablesGroup1 {
                clkfreq: raw.clkfreq,
                uptime,
                thrust,
            }
        }

        pub fn transform_og2(&self, raw: &RawObservablesGroup2) -> ObservablesGroup2 {
            let anomalies = raw.anomalies;
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
            }
        }
    }
}

impl Timestamp {
    pub fn duration(&self, clkfreq: &ClkFreq) -> Duration {
        let clkfreq = clkfreq.0 as u64;
        let secs = Duration::from_secs(self.0 / clkfreq);
        let rest = self.0 % clkfreq;
        let nanos = rest * 1000_000_000 / clkfreq;
        secs + Duration::from_nanos(nanos)
    }
}

impl Into<f64> for Ads1256Reading {
    fn into(self) -> f64 {
        self.0 as f64 / 0x800000 as f64
    }
}
impl AdcWeightCalibration {
    pub fn weight(&self, value: impl Into<f64>) -> Mass {
        let res = value.into() * self.m + self.c;
        Mass::new::<gram>(res)
    }
}
#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_duration_from_timestamp() {
        let clkfreq = ClkFreq(300_000_000);
        let ts = Timestamp(600_000_000);
        assert_eq!(ts.duration(&clkfreq), Duration::from_secs(2));
        let ts = Timestamp(600_000_300);
        assert_eq!(
            ts.duration(&clkfreq),
            Duration::from_secs(2) + Duration::from_micros(1)
        );
    }

    #[test]
    fn test_weight_from_adc_reading() {
        let reading = Ads1256Reading(433110);
        let (m, c) = (127539.14190327494, -6423.647555776099);
        let calibration = AdcWeightCalibration { m, c };
        let g160 = Mass::new::<gram>(161.29213263554357);
        assert_eq!(calibration.weight(reading), g160);
    }
}
