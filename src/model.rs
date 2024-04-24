use log::{debug, error};
#[cfg(test)]
use mock_instant::Instant;
use ringbuffer::{AllocRingBuffer, RingBuffer};
#[cfg(not(test))]
use std::time::Instant;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    connection::{Answers, Connection},
    consort::{Consort, SimpleIdGenerator},
    input::InputEvent,
    observables::{
        rqa::{ObservablesGroup1, ObservablesGroup2, RawObservablesGroup, SystemDefinition},
        AdcGain,
    },
    rqparser::MAX_BUFFER_SIZE,
    rqprotocol::{Command, Response},
};

#[derive(Clone)]
pub struct SharedIdGenerator {
    command_id_generator: Arc<Mutex<SimpleIdGenerator>>,
}

impl Iterator for SharedIdGenerator {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        self.command_id_generator.lock().unwrap().next()
    }
}

impl Default for SharedIdGenerator {
    fn default() -> Self {
        Self {
            command_id_generator: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaunchControlState {
    Start,
    Failure,
    Reset,
    Idle,
    EnterDigitHiA {
        hi_a: u8,
    },
    EnterDigitLoA {
        hi_a: u8,
        lo_a: u8,
    },
    TransmitKeyA {
        hi_a: u8,
        lo_a: u8,
    },
    PrepareUnlockPyros {
        hi_a: u8,
        lo_a: u8,
        progress: u8,
        last_update: Instant,
    },
    UnlockPyros {
        hi_a: u8,
        lo_a: u8,
    },
    EnterDigitHiB {
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
    },
    EnterDigitLoB {
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
    },
    TransmitKeyAB {
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
    },
    PrepareIgnition {
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
        progress: u8,
        last_update: Instant,
    },
    WaitForFire {
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
    },
    Fire,
    WaitForPyroTimeout(Instant),
    SwitchToObservables,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObservablesState {
    Start,
    Failure,
    Reset,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Observables(ObservablesState),
    LaunchControl(LaunchControlState),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlArea {
    Tabs,
    Details,
}

pub struct Model<C, Id>
where
    C: Connection,
    Id: Iterator<Item = usize>,
{
    pub mode: Mode,
    pub control: ControlArea,
    pub consort: Consort<Id>,
    module: C,
    start: Instant,
    now: Instant,
    port: String,
    pub obg1: Vec<ObservablesGroup1>,
    pub obg2: Option<ObservablesGroup2>,
    pub established_connection_at: Option<Instant>,
    pub adc_gain: AdcGain,
    pub recorder_path: Option<PathBuf>,
}

pub trait StateProcessing {
    type State;

    fn process_response(&self, response: Response) -> Self::State;

    fn name(&self) -> &str;

    fn is_failure(&self) -> bool;

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea);

    fn process_mode_change(&self) -> Option<Command>;

    fn drive(&self) -> Self::State;

    fn connected(&self) -> bool;

    fn is_start(&self) -> bool;

    fn reset_mode(&self) -> Self::State;

    fn failure_mode(&self) -> Self::State;

    fn reset_ongoing(&self) -> bool;
}

impl StateProcessing for LaunchControlState {
    type State = LaunchControlState;

    fn process_response(&self, response: Response) -> Self::State {
        match self {
            Self::State::Reset => match response {
                Response::ResetAck => {
                    debug!("Acknowledged Reset, go to Idle");
                    Self::State::Idle
                }
                _ => Self::State::Start,
            },
            Self::State::Idle => *self,
            Self::TransmitKeyA { hi_a, lo_a } => match response {
                Response::LaunchSecretPartialAck => Self::PrepareUnlockPyros {
                    hi_a: *hi_a,
                    lo_a: *lo_a,
                    progress: 0,
                    last_update: Instant::now(),
                },
                _ => Self::State::Start,
            },
            Self::UnlockPyros { hi_a, lo_a, .. } => match response {
                Response::UnlockPyrosAck => Self::State::EnterDigitHiB {
                    hi_a: *hi_a,
                    lo_a: *lo_a,
                    hi_b: 0,
                },
                _ => Self::State::Start,
            },
            Self::TransmitKeyAB {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => match response {
                Response::LaunchSecretFullAck => Self::State::PrepareIgnition {
                    hi_a: *hi_a,
                    lo_a: *lo_a,
                    hi_b: *hi_b,
                    lo_b: *lo_b,
                    progress: 0,
                    last_update: Instant::now(),
                },
                _ => Self::State::Start,
            },
            Self::State::Fire => match response {
                Response::IgnitionAck => Self::State::WaitForPyroTimeout(Instant::now()),
                _ => Self::State::Start,
            },
            _ => *self,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::State::Start => "Start",
            Self::State::Failure => "Failure",
            Self::State::Reset => "Reset",
            Self::State::Idle => "Idle",
            Self::State::EnterDigitHiA { .. } => "Enter Hi A",
            Self::State::EnterDigitLoA { .. } => "Enter Lo A",
            Self::State::PrepareUnlockPyros { .. } => "Prepare Unlock Pyros",
            Self::State::UnlockPyros { .. } => "Unlocking Pyros",
            Self::State::TransmitKeyA { .. } => "Transmitting Key A",
            Self::State::EnterDigitHiB { .. } => "Enter Hi B",
            Self::State::EnterDigitLoB { .. } => "Enter Lo B",
            Self::State::TransmitKeyAB { .. } => "Transmitting Key AB",
            Self::State::PrepareIgnition { .. } => "Prepare Ignition",
            Self::State::WaitForFire { .. } => "Wait for Fire",
            Self::State::Fire => "Fire!",
            Self::State::WaitForPyroTimeout { .. } => "Pyros ignited",
            Self::State::SwitchToObservables => "",
        }
    }

    fn is_failure(&self) -> bool {
        match self {
            Self::State::Failure => true,
            _ => false,
        }
    }

    fn is_start(&self) -> bool {
        match self {
            Self::State::Start => true,
            _ => false,
        }
    }

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea) {
        match self {
            LaunchControlState::Idle => self.process_event_idle(event),
            LaunchControlState::EnterDigitHiA { hi_a } => {
                self.process_event_enter_higit_hi_a(event, *hi_a)
            }
            LaunchControlState::EnterDigitLoA { hi_a, lo_a } => {
                self.process_event_enter_higit_lo_a(event, *hi_a, *lo_a)
            }
            LaunchControlState::EnterDigitHiB { hi_a, lo_a, hi_b } => {
                self.process_event_enter_higit_hi_b(event, *hi_a, *lo_a, *hi_b)
            }
            LaunchControlState::EnterDigitLoB {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => self.process_event_enter_higit_lo_b(event, *hi_a, *lo_a, *hi_b, *lo_b),
            LaunchControlState::PrepareIgnition {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
                progress,
                last_update,
            } => self.process_prepare_ignition(
                event,
                *hi_a,
                *lo_a,
                *hi_b,
                *lo_b,
                *progress,
                *last_update,
            ),
            LaunchControlState::PrepareUnlockPyros {
                hi_a,
                lo_a,
                progress,
                last_update,
            } => self.process_unlock_pyros(event, *hi_a, *lo_a, *progress, *last_update),
            LaunchControlState::WaitForFire {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => self.process_fire(event, *hi_a, *lo_a, *hi_b, *lo_b),
            // only left through a response
            LaunchControlState::TransmitKeyA { .. } => (*self, ControlArea::Details),
            // only left through a response
            LaunchControlState::TransmitKeyAB { .. } => (*self, ControlArea::Details),
            // only left through a response
            LaunchControlState::Fire => (*self, ControlArea::Details),
            // only left through a response
            LaunchControlState::UnlockPyros { .. } => (*self, ControlArea::Details),
            _ => self.process_event_nop(event),
        }
    }

    fn process_mode_change(&self) -> Option<Command> {
        match self {
            LaunchControlState::TransmitKeyA { hi_a, lo_a } => {
                Some(Command::LaunchSecretPartial(hi_a << 4 | lo_a))
            }
            LaunchControlState::TransmitKeyAB {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => Some(Command::LaunchSecretFull(
                hi_a << 4 | lo_a,
                hi_b << 4 | lo_b,
            )),
            LaunchControlState::Fire => Some(Command::Ignition),
            LaunchControlState::UnlockPyros { .. } => Some(Command::UnlockPyros),
            _ => None,
        }
    }

    fn drive(&self) -> Self {
        match self {
            LaunchControlState::PrepareIgnition {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
                progress,
                last_update,
            } => LaunchControlState::PrepareIgnition {
                hi_a: *hi_a,
                lo_a: *lo_a,
                hi_b: *hi_b,
                lo_b: *lo_b,
                progress: if *progress < 100 {
                    if last_update.elapsed() > Duration::from_millis(500) {
                        std::cmp::max(*progress, 1) - 1
                    } else {
                        *progress
                    }
                } else {
                    100
                },
                last_update: *last_update,
            },
            LaunchControlState::PrepareUnlockPyros {
                hi_a,
                lo_a,
                progress,
                last_update,
            } => LaunchControlState::PrepareUnlockPyros {
                hi_a: *hi_a,
                lo_a: *lo_a,
                progress: if *progress < 100 {
                    if last_update.elapsed() > Duration::from_millis(500) {
                        std::cmp::max(*progress, 1) - 1
                    } else {
                        *progress
                    }
                } else {
                    100
                },
                last_update: *last_update,
            },
            LaunchControlState::WaitForPyroTimeout(timeout) => {
                if timeout.elapsed() > Duration::from_secs(3) {
                    LaunchControlState::SwitchToObservables
                } else {
                    *self
                }
            }
            _ => *self,
        }
    }

    fn connected(&self) -> bool {
        match self {
            LaunchControlState::Start => false,
            LaunchControlState::Failure => false,
            LaunchControlState::Reset => false,
            _ => true,
        }
    }

    fn reset_mode(&self) -> Self::State {
        Self::State::Reset
    }

    fn failure_mode(&self) -> Self::State {
        Self::State::Failure
    }

    fn reset_ongoing(&self) -> bool {
        match self {
            LaunchControlState::Start => true,
            LaunchControlState::Reset => true,
            _ => false,
        }
    }
}

impl StateProcessing for ObservablesState {
    type State = ObservablesState;

    fn process_response(&self, response: Response) -> Self::State {
        match self {
            ObservablesState::Failure => todo!(),
            ObservablesState::Reset => match response {
                Response::ResetAck => Self::State::Idle,
                _ => Self::State::Start,
            },
            _ => *self,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::State::Start => "Start",
            Self::State::Failure => "Failure",
            Self::State::Reset => "Reset",
            Self::State::Idle => "Idle",
        }
    }

    fn is_failure(&self) -> bool {
        match self {
            Self::State::Failure => true,
            _ => false,
        }
    }

    fn is_start(&self) -> bool {
        match self {
            Self::State::Start => true,
            _ => false,
        }
    }

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea) {
        match event {
            InputEvent::Back => (*self, ControlArea::Tabs),
            _ => (*self, ControlArea::Details),
        }
    }

    fn process_mode_change(&self) -> Option<Command> {
        None
    }

    fn drive(&self) -> Self {
        *self
    }

    fn connected(&self) -> bool {
        match self {
            ObservablesState::Start => false,
            ObservablesState::Failure => false,
            ObservablesState::Reset => false,
            _ => true,
        }
    }

    fn reset_mode(&self) -> Self::State {
        Self::State::Reset
    }

    fn failure_mode(&self) -> Self::State {
        Self::State::Failure
    }

    fn reset_ongoing(&self) -> bool {
        match self {
            Self::Start => true,
            Self::Reset => true,
            _ => false,
        }
    }
}

impl Default for LaunchControlState {
    fn default() -> Self {
        Self::Start
    }
}

impl Default for ObservablesState {
    fn default() -> Self {
        Self::Start
    }
}

impl StateProcessing for Mode {
    type State = Mode;

    fn process_response(&self, response: Response) -> Self::State {
        match self {
            Mode::Observables(state) => Mode::Observables(state.process_response(response)),
            Mode::LaunchControl(state) => Mode::LaunchControl(state.process_response(response)),
        }
    }

    fn name(&self) -> &str {
        match self {
            Mode::Observables(state) => state.name(),
            Mode::LaunchControl(state) => state.name(),
        }
    }

    fn is_failure(&self) -> bool {
        match self {
            Mode::Observables(state) => state.is_failure(),
            Mode::LaunchControl(state) => state.is_failure(),
        }
    }

    fn is_start(&self) -> bool {
        match self {
            Mode::Observables(state) => state.is_start(),
            Mode::LaunchControl(state) => state.is_start(),
        }
    }

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea) {
        match self {
            Mode::Observables(state) => {
                let (state, ca) = state.process_event(event);
                (Mode::Observables(state), ca)
            }
            Mode::LaunchControl(state) => {
                let (state, ca) = state.process_event(event);
                (Mode::LaunchControl(state), ca)
            }
        }
    }

    fn process_mode_change(&self) -> Option<Command> {
        match self {
            Mode::LaunchControl(state) => state.process_mode_change(),
            Mode::Observables(state) => state.process_mode_change(),
        }
    }

    fn drive(&self) -> Self {
        let mut mode = match self {
            Mode::LaunchControl(state) => Mode::LaunchControl(state.drive()),
            Mode::Observables(state) => Mode::Observables(state.drive()),
        };
        if let Mode::LaunchControl(LaunchControlState::SwitchToObservables) = mode {
            mode = Mode::Observables(ObservablesState::Start)
        }
        mode
    }

    fn connected(&self) -> bool {
        match self {
            Mode::Observables(state) => state.connected(),
            Mode::LaunchControl(state) => state.connected(),
        }
    }

    fn reset_mode(&self) -> Self::State {
        match self {
            Mode::Observables(state) => Mode::Observables(state.reset_mode()),
            Mode::LaunchControl(state) => Mode::LaunchControl(state.reset_mode()),
        }
    }

    fn failure_mode(&self) -> Self::State {
        match self {
            Mode::Observables(state) => Mode::Observables(state.failure_mode()),
            Mode::LaunchControl(state) => Mode::LaunchControl(state.failure_mode()),
        }
    }

    fn reset_ongoing(&self) -> bool {
        match self {
            Mode::Observables(state) => state.reset_ongoing(),
            Mode::LaunchControl(state) => state.reset_ongoing(),
        }
    }
}

impl Default for ControlArea {
    fn default() -> Self {
        Self::Tabs
    }
}

impl LaunchControlState {
    pub fn digits(&self) -> (u8, u8, u8, u8) {
        match self {
            LaunchControlState::Start => (0, 0, 0, 0),
            LaunchControlState::Failure => (0, 0, 0, 0),
            LaunchControlState::Reset => (0, 0, 0, 0),
            LaunchControlState::Idle => (0, 0, 0, 0),
            LaunchControlState::EnterDigitHiA { hi_a } => (*hi_a, 0, 0, 0),
            LaunchControlState::EnterDigitLoA { hi_a, lo_a } => (*hi_a, *lo_a, 0, 0),
            LaunchControlState::PrepareUnlockPyros { hi_a, lo_a, .. } => (*hi_a, *lo_a, 0, 0),
            LaunchControlState::UnlockPyros { hi_a, lo_a, .. } => (*hi_a, *lo_a, 0, 0),
            LaunchControlState::TransmitKeyA { hi_a, lo_a } => (*hi_a, *lo_a, 0, 0),
            LaunchControlState::EnterDigitHiB { hi_a, lo_a, hi_b } => (*hi_a, *lo_a, *hi_b, 0),
            LaunchControlState::EnterDigitLoB {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => (*hi_a, *lo_a, *hi_b, *lo_b),
            LaunchControlState::TransmitKeyAB {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => (*hi_a, *lo_a, *hi_b, *lo_b),
            LaunchControlState::PrepareIgnition {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
                ..
            } => (*hi_a, *lo_a, *hi_b, *lo_b),
            LaunchControlState::WaitForFire {
                hi_a,
                lo_a,
                hi_b,
                lo_b,
            } => (*hi_a, *lo_a, *hi_b, *lo_b),
            LaunchControlState::Fire => (0, 0, 0, 0),
            LaunchControlState::WaitForPyroTimeout(_) => (0, 0, 0, 0),
            LaunchControlState::SwitchToObservables => (0, 0, 0, 0),
        }
    }

    pub fn highlights(&self) -> (bool, bool, bool, bool) {
        match self {
            LaunchControlState::EnterDigitHiA { .. } => (true, false, false, false),
            LaunchControlState::EnterDigitLoA { .. } => (false, true, false, false),
            LaunchControlState::EnterDigitHiB { .. } => (false, false, true, false),
            LaunchControlState::EnterDigitLoB { .. } => (false, false, false, true),
            _ => (false, false, false, false),
        }
    }

    pub fn prepare_ignition_progress(&self) -> f32 {
        let p = match self {
            LaunchControlState::PrepareIgnition { progress, .. } => *progress,
            LaunchControlState::WaitForFire { .. } => 100,
            _ => 0,
        };
        p as f32 / 100.0
    }

    pub fn unlock_pyros_progress(&self) -> f32 {
        let p = match self {
            LaunchControlState::Start => 0,
            LaunchControlState::Failure => 0,
            LaunchControlState::Reset => 0,
            LaunchControlState::Idle => 0,
            LaunchControlState::EnterDigitHiA { .. } => 0,
            LaunchControlState::EnterDigitLoA { .. } => 0,
            LaunchControlState::TransmitKeyA { .. } => 0,
            LaunchControlState::PrepareUnlockPyros { progress, .. } => *progress,
            _ => 100,
        };
        p as f32 / 100.0
    }

    fn process_event_nop(&self, _event: &InputEvent) -> (Self, ControlArea) {
        // States Start, Failure, Idle are not input dependent
        (*self, ControlArea::Tabs)
    }

    fn process_event_idle(&self, event: &InputEvent) -> (Self, ControlArea) {
        match event {
            InputEvent::Enter => (
                LaunchControlState::EnterDigitHiA { hi_a: 0 },
                ControlArea::Details,
            ),
            _ => self.process_event_nop(event),
        }
    }

    fn process_event_enter_higit_hi_a(&self, event: &InputEvent, digit: u8) -> (Self, ControlArea) {
        match event {
            InputEvent::Enter => (
                LaunchControlState::EnterDigitLoA {
                    hi_a: digit,
                    lo_a: 0,
                },
                ControlArea::Details,
            ),
            InputEvent::Back => (LaunchControlState::Start, ControlArea::Tabs),
            InputEvent::Right(_) => (
                LaunchControlState::EnterDigitHiA {
                    hi_a: (digit + 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Left(_) => (
                LaunchControlState::EnterDigitHiA {
                    hi_a: (16 + digit - 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Send => self.process_event_nop(event),
        }
    }

    fn process_event_enter_higit_lo_a(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
    ) -> (Self, ControlArea) {
        match event {
            InputEvent::Enter => (
                LaunchControlState::TransmitKeyA { hi_a, lo_a },
                ControlArea::Details,
            ),
            // Back to high digit!
            InputEvent::Back => (
                LaunchControlState::EnterDigitHiA { hi_a },
                ControlArea::Details,
            ),
            InputEvent::Right(_) => (
                LaunchControlState::EnterDigitLoA {
                    hi_a,
                    lo_a: (lo_a + 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Left(_) => (
                LaunchControlState::EnterDigitLoA {
                    hi_a,
                    lo_a: (16 + lo_a - 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Send => self.process_event_nop(event),
        }
    }

    fn process_event_enter_higit_hi_b(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
    ) -> (Self, ControlArea) {
        match event {
            InputEvent::Enter => (
                LaunchControlState::EnterDigitLoB {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b: 0,
                },
                ControlArea::Details,
            ),
            InputEvent::Back => (LaunchControlState::Start, ControlArea::Tabs),
            InputEvent::Right(_) => (
                LaunchControlState::EnterDigitHiB {
                    hi_a,
                    lo_a,
                    hi_b: (hi_b + 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Left(_) => (
                LaunchControlState::EnterDigitHiB {
                    hi_a,
                    lo_a,
                    hi_b: (16 + hi_b - 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Send => self.process_event_nop(event),
        }
    }

    fn process_event_enter_higit_lo_b(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
    ) -> (Self, ControlArea) {
        match event {
            InputEvent::Enter => (
                LaunchControlState::TransmitKeyAB {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b,
                },
                ControlArea::Details,
            ),
            // Back to the high digit!
            InputEvent::Back => (
                LaunchControlState::EnterDigitHiB { hi_a, lo_a, hi_b },
                ControlArea::Details,
            ),
            InputEvent::Right(_) => (
                LaunchControlState::EnterDigitLoB {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b: (lo_b + 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Left(_) => (
                LaunchControlState::EnterDigitLoB {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b: (16 + lo_b - 1) % 16,
                },
                ControlArea::Details,
            ),
            InputEvent::Send => self.process_event_nop(event),
        }
    }

    fn process_prepare_ignition(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
        progress: u8,
        last_update: Instant,
    ) -> (Self, ControlArea) {
        let now = Instant::now();
        if progress == 100 {
            (
                LaunchControlState::WaitForFire {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b,
                },
                ControlArea::Details,
            )
        } else {
            match event {
                InputEvent::Back => (LaunchControlState::Start, ControlArea::Tabs),
                InputEvent::Right(_) => (
                    LaunchControlState::PrepareIgnition {
                        hi_a,
                        lo_a,
                        hi_b,
                        lo_b,
                        progress: std::cmp::min(progress + 3, 100),
                        last_update: now,
                    },
                    ControlArea::Details,
                ),
                _ => (
                    LaunchControlState::PrepareIgnition {
                        hi_a,
                        lo_a,
                        hi_b,
                        lo_b,
                        progress,
                        last_update,
                    },
                    ControlArea::Details,
                ),
            }
        }
    }

    fn process_unlock_pyros(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
        progress: u8,
        last_update: Instant,
    ) -> (Self, ControlArea) {
        let now = Instant::now();
        if progress == 100 {
            (
                LaunchControlState::UnlockPyros { hi_a, lo_a },
                ControlArea::Details,
            )
        } else {
            match event {
                InputEvent::Back => (LaunchControlState::Start, ControlArea::Tabs),
                InputEvent::Right(_) => (
                    LaunchControlState::PrepareUnlockPyros {
                        hi_a,
                        lo_a,
                        progress: std::cmp::min(progress + 3, 100),
                        last_update: now,
                    },
                    ControlArea::Details,
                ),
                _ => (
                    LaunchControlState::PrepareUnlockPyros {
                        hi_a,
                        lo_a,
                        progress,
                        last_update,
                    },
                    ControlArea::Details,
                ),
            }
        }
    }

    fn process_fire(
        &self,
        event: &InputEvent,
        hi_a: u8,
        lo_a: u8,
        hi_b: u8,
        lo_b: u8,
    ) -> (Self, ControlArea) {
        match event {
            InputEvent::Back => (LaunchControlState::Start, ControlArea::Tabs),
            InputEvent::Enter => (LaunchControlState::Fire, ControlArea::Details),
            _ => (
                LaunchControlState::WaitForFire {
                    hi_a,
                    lo_a,
                    hi_b,
                    lo_b,
                },
                ControlArea::Details,
            ),
        }
    }
}

impl<C: Connection, Id: Iterator<Item = usize>> Model<C, Id> {
    pub fn new(
        consort: Consort<Id>,
        module: C,
        now: Instant,
        port: &str,
        gain: &AdcGain,
        start_with_launch_control: bool,
        recorder_path: Option<PathBuf>,
    ) -> Self {
        Self {
            mode: if start_with_launch_control {
                Mode::LaunchControl(LaunchControlState::default())
            } else {
                Mode::Observables(ObservablesState::default())
            },
            control: Default::default(),
            consort,
            start: now,
            now,
            module,
            port: port.into(),
            obg1: vec![],
            obg2: None,
            established_connection_at: None,
            adc_gain: gain.clone(),
            recorder_path,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn drive(&mut self, now: Instant) -> anyhow::Result<()> {
        self.now = now;
        self.consort.update_time(now);
        // When we are in start state, start a reset cycle
        if self.mode.is_start() {
            self.reset();
            return Ok(());
        }

        let mut ringbuffer = AllocRingBuffer::new(MAX_BUFFER_SIZE);
        let mut timeout = false;
        let mut error = false;
        let mut reset = false;
        let mut observables = None;
        self.module.recv(|answer| match answer {
            Answers::Received(sentence) => {
                for c in sentence {
                    ringbuffer.push(c);
                }
            }
            Answers::Timeout => {
                timeout = true;
            }
            Answers::ConnectionError => {
                error = true;
            }
            Answers::Observables(o) => {
                observables = Some(o);
            }
            Answers::Drained => {
                reset = true;
            }
            Answers::ConnectionOpen => {
                // Go through a reset cycle on a new connection
                reset = true;
            }
        });
        if let Some(o) = observables {
            self.process_observables(&o);
        }
        if timeout {
            self.module.drain();
            self.obg1.clear();
            self.obg2 = None;
        } else if reset {
            self.reset();
        } else if error {
            self.mode = self.mode.failure_mode();
            self.module.open(&self.port);
        } else {
            while !ringbuffer.is_empty() {
                match self.consort.feed(&mut ringbuffer) {
                    Ok(response) => {
                        if let Some(response) = response {
                            debug!("process_response: {:?}", response);
                            self.process_response(response);
                        }
                        self.module.resume();
                    }
                    Err(err) => {
                        error!("Feeding consort error: {:?}", err);
                        self.module.reset();
                        self.module.drain();
                        break;
                    }
                }
            }
        }
        self.set_mode(self.mode.drive());
        Ok(())
    }

    fn reset(&mut self) {
        self.mode = self.mode.reset_mode();
        self.established_connection_at = None;
        self.consort.reset();
        self.module.reset();
        match self
            .consort
            .send_command(Command::Reset(self.adc_gain.clone()), &mut self.module)
        {
            Ok(_) => {}
            Err(_) => {
                self.mode = self.mode.failure_mode();
            }
        }
    }

    fn process_response(&mut self, response: Response) {
        if let Response::ObservableGroup(raw_observables) = response {
            self.process_observables(&raw_observables)
        } else {
            self.set_mode(self.mode.process_response(response));
        }
    }

    fn process_observables(&mut self, raw: &RawObservablesGroup) {
        let sys_def = SystemDefinition::default();
        match raw {
            RawObservablesGroup::OG1(obg1) => {
                self.obg1.push(sys_def.transform_og1(obg1));
            }
            RawObservablesGroup::OG2(obg2) => {
                self.obg2 = Some(sys_def.transform_og2(obg2));
            }
        }
    }

    pub fn process_input_events(&mut self, events: &Vec<InputEvent>) {
        for event in events {
            self.process_input_event(event);
        }
    }

    fn process_input_event(&mut self, event: &InputEvent) {
        self.control = match self.control {
            ControlArea::Tabs => self.process_tabs_event(event),
            ControlArea::Details => self.process_details_event(event),
        };
    }

    fn process_tabs_event(&mut self, event: &InputEvent) -> ControlArea {
        match event {
            InputEvent::Left(..) => self.toggle_tab(),
            InputEvent::Right(..) => self.toggle_tab(),
            InputEvent::Enter => {
                let (mode, control) = self.mode.process_event(event);
                self.mode = mode;
                control
            }
            _ => self.control,
        }
    }

    fn process_details_event(&mut self, event: &InputEvent) -> ControlArea {
        debug!("process_detail_event: {:?}", event);
        let (mode, control_area) = self.mode.process_event(event);
        self.set_mode(mode);
        control_area
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            debug!("old mode: {:?}, new mode: {:?}", self.mode, mode);
            self.mode = mode;
            self.process_mode_change();
        }
    }

    fn process_mode_change(&mut self) {
        if let Some(command) = self.mode.process_mode_change() {
            if self
                .consort
                .send_command(command, &mut self.module)
                .is_err()
            {
                self.reset();
            }
        }
        match self.established_connection_at {
            Some(_) => {
                if !self.connected() {
                    self.established_connection_at = None
                }
            }
            None => {
                if self.connected() {
                    self.established_connection_at = Some(Instant::now());
                }
            }
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        self.established_connection_at
            .and_then(|timepoint| Some(Instant::now() - timepoint))
    }

    fn toggle_tab(&mut self) -> ControlArea {
        if !self.mode.reset_ongoing() {
            self.mode = match self.mode {
                Mode::LaunchControl(_) => Mode::Observables(ObservablesState::Start),
                Mode::Observables(_) => Mode::LaunchControl(LaunchControlState::Start),
            }
        }
        ControlArea::Tabs
    }

    pub fn connected(&self) -> bool {
        self.mode.connected()
    }
}

#[cfg(test)]
mod tests {
    use crate::consort::SimpleIdGenerator;
    use crate::rqparser::command_parser;
    use crate::rqprotocol::Node;
    use std::assert_matches::assert_matches;

    use super::*;

    struct MockConnection {
        responses: Vec<Vec<u8>>,
    }

    impl Connection for MockConnection {
        fn recv(&mut self, callback: impl FnOnce(Answers)) {
            if self.responses.len() > 0 {
                let response = self.responses.pop().unwrap();
                callback(Answers::Received(response));
            }
        }

        fn drain(&mut self) {
            todo!()
        }

        fn open(&mut self, _port: &str) {
            todo!()
        }

        fn reset(&mut self) {
            todo!()
        }

        fn resume(&mut self) {
            todo!()
        }
    }

    impl std::io::Write for MockConnection {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match command_parser(&buf[1..buf.len() - 4]) {
                Ok((.., transaction)) => {
                    let mut buffer = [0; MAX_BUFFER_SIZE];
                    let response = transaction.acknowledge(&mut buffer).unwrap();
                    self.responses.push(response.into());
                    Ok(buf.len())
                }
                Err(_) => unreachable!("We should never receive wrong commands"),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_full_fsm_progression() {
        let connection = MockConnection { responses: vec![] };
        let now = Instant::now();
        let consort = Consort::new_with_id_generator(
            Node::LaunchControl,
            Node::RedQueen(b'A'),
            now,
            SimpleIdGenerator::default(),
        );
        let mut model = Model::new(consort, connection, now, "comport", &AdcGain::Gain64);
        assert_matches!(model.mode(), Mode::LaunchControl(_));
        assert_eq!(model.control, ControlArea::Tabs);
        // Put us into reset
        model.drive(Instant::now()).unwrap();
        // progress to idle
        model.drive(Instant::now()).unwrap();
        assert_matches!(model.mode(), Mode::LaunchControl(LaunchControlState::Idle));
        model.process_input_event(&InputEvent::Enter);
        assert_eq!(model.control, ControlArea::Details);
        assert_matches!(model.mode(), Mode::LaunchControl(_));
    }
}
