use clap::ArgEnum;
use std::time::Duration;
use uom::si::f64::*;
use uom::si::force::kilonewton;
use uom::si::pressure::bar;

// Raw wire-values
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ClkFreq(pub u32);

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Timestamp(pub u64);

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Ads1256Reading(pub i32);

struct AdcForceCalibration {
    m: f64,
    c: f64,
}

struct AdcPressureCalibration {
    m: f64,
    c: f64,
}

#[derive(Clone, Debug, ArgEnum, PartialEq)]
#[clap(rename_all = "kebab_case")]
pub enum AdcGain {
    Gain1,
    Gain2,
    Gain4,
    Gain8,
    Gain16,
    Gain32,
    Gain64,
}

#[cfg(feature = "test-stand")]
pub mod rqa;
#[cfg(feature = "rocket")]
pub mod rqb;

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
        self.0 as f64
    }
}

impl AdcForceCalibration {
    pub fn force(&self, value: impl Into<f64>) -> Force {
        let res = value.into() * self.m + self.c;
        Force::new::<kilonewton>(res)
    }
}

impl AdcPressureCalibration {
    pub fn pressure(&self, value: impl Into<f64>) -> Pressure {
        let res = value.into() * self.m + self.c;
        Pressure::new::<bar>(res)
    }
}

impl Into<u8> for AdcGain {
    fn into(self) -> u8 {
        match self {
            AdcGain::Gain1 => 1,
            AdcGain::Gain2 => 2,
            AdcGain::Gain4 => 4,
            AdcGain::Gain8 => 8,
            AdcGain::Gain16 => 16,
            AdcGain::Gain32 => 32,
            AdcGain::Gain64 => 64,
        }
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
        let calibration = AdcForceCalibration { m, c };
        // let f = Force::new::<kilonewton>(5523847132607986.0);
        // assert_eq!(calibration.force(reading), f);
    }
}
