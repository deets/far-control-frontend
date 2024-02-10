use log::{debug, error};
#[cfg(test)]
use mock_instant::Instant;
use ringbuffer::{AllocRingBuffer, RingBuffer};
#[cfg(not(test))]
use std::time::Instant;

use std::time::Duration;

use crate::{
    consort::Consort,
    ebyte::E32Connection,
    input::InputEvent,
    rqparser::MAX_BUFFER_SIZE,
    rqprotocol::{Command, Response},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LaunchControlState {
    Start,
    Failure,
    Reset,
    Idle,
    EnterDigitHiA { hi_a: u8 },
    EnterDigitLoA { hi_a: u8, lo_a: u8 },
    TransmitSecretA { hi_a: u8, lo_a: u8 },
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

pub struct Model<'a> {
    pub mode: Mode,
    pub control: ControlArea,
    pub consort: Consort<'a>,
    module: E32Connection,
    start: Instant,
    now: Instant,
}

pub trait StateProcessing {
    type State;

    fn process_response(&self, response: Response) -> Self::State;

    fn name(&self) -> &str;

    fn is_failure(&self) -> bool;

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea);

    fn process_mode_change(&self) -> Option<Command>;
}

impl StateProcessing for LaunchControlState {
    type State = LaunchControlState;

    fn process_response(&self, response: Response) -> Self::State {
        match self {
            Self::State::Start => *self,
            Self::State::Failure => *self,
            Self::State::Reset => match response {
                Response::ResetAck => {
                    debug!("Acknowledged Reset, go to Idle");
                    Self::State::Idle
                }
                _ => Self::State::Start,
            },
            Self::State::Idle => *self,
            Self::EnterDigitHiA { .. } => *self,
            Self::EnterDigitLoA { .. } => *self,
            Self::TransmitSecretA { .. } => *self,
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
            Self::State::TransmitSecretA { .. } => "Transmitting Secret A",
        }
    }

    fn is_failure(&self) -> bool {
        match self {
            Self::State::Failure => true,
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
            // only left through a response
            LaunchControlState::TransmitSecretA { .. } => (*self, ControlArea::Details),
            _ => self.process_event_nop(event),
        }
    }

    fn process_mode_change(&self) -> Option<Command> {
        match self {
            LaunchControlState::TransmitSecretA { hi_a, lo_a } => {
                Some(Command::LaunchSecretPartial(hi_a << 4 | lo_a))
            }
            _ => None,
        }
    }
}

impl StateProcessing for ObservablesState {
    type State = ObservablesState;

    fn process_response(&self, response: Response) -> Self::State {
        *self
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

    fn process_event(&self, event: &InputEvent) -> (Self::State, ControlArea) {
        match event {
            InputEvent::Back => (*self, ControlArea::Tabs),
            _ => (*self, ControlArea::Details),
        }
    }

    fn process_mode_change(&self) -> Option<Command> {
        None
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
}

impl Default for Mode {
    fn default() -> Self {
        Self::LaunchControl(LaunchControlState::Start)
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
            LaunchControlState::TransmitSecretA { hi_a, lo_a } => (*hi_a, *lo_a, 0, 0),
        }
    }

    pub fn highlights(&self) -> (bool, bool, bool, bool) {
        match self {
            LaunchControlState::EnterDigitHiA { .. } => (true, false, false, false),
            LaunchControlState::EnterDigitLoA { .. } => (false, true, false, false),
            _ => (false, false, false, false),
        }
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
            InputEvent::Back => (LaunchControlState::Idle, ControlArea::Tabs),
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
                LaunchControlState::TransmitSecretA { hi_a, lo_a },
                ControlArea::Details,
            ),
            InputEvent::Back => (LaunchControlState::Idle, ControlArea::Tabs),
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
}

impl<'a> Model<'a> {
    pub fn new(consort: Consort<'a>, module: E32Connection, now: Instant) -> Self {
        Self {
            mode: Default::default(),
            control: Default::default(),
            consort,
            start: now,
            now,
            module,
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
        match self.mode {
            Mode::Observables(ObservablesState::Start) => {
                debug!("Resetting because we are in Start");
                self.reset();
                return Ok(());
            }
            Mode::LaunchControl(LaunchControlState::Start) => {
                debug!("Resetting because we are in Start");
                self.reset();
                return Ok(());
            }
            _ => {}
        }

        let mut ringbuffer = AllocRingBuffer::new(MAX_BUFFER_SIZE);
        let mut timeout = false;
        self.module.recv(|answer| match answer {
            crate::ebyte::Answers::Received(sentence) => {
                for c in sentence {
                    ringbuffer.push(c);
                }
            }
            crate::ebyte::Answers::Timeout => {
                timeout = true;
            }
        });

        if timeout {
            self.reset();
        } else {
            while !ringbuffer.is_empty() {
                match self.consort.feed(&mut ringbuffer) {
                    Ok(response) => {
                        if let Some(response) = response {
                            self.process_response(response);
                        }
                    }
                    Err(err) => {
                        error!("Feeding consort error: {:?}", err);
                        self.reset();
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.mode = match self.mode {
            Mode::Observables(_) => Mode::Observables(ObservablesState::Reset),
            Mode::LaunchControl(_) => Mode::LaunchControl(LaunchControlState::Reset),
        };
        self.consort.reset();
        match self.consort.send_command(Command::Reset, &mut self.module) {
            Ok(_) => {}
            Err(_) => {
                error!("Resetting failed");
                self.mode = match self.mode {
                    Mode::Observables(_) => Mode::Observables(ObservablesState::Failure),
                    Mode::LaunchControl(_) => Mode::LaunchControl(LaunchControlState::Failure),
                }
            }
        }
    }

    fn process_response(&mut self, response: Response) {
        self.mode = self.mode.process_response(response);
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
            InputEvent::Enter => self.mode.process_event(event).1,
            _ => self.control,
        }
    }

    fn process_details_event(&mut self, event: &InputEvent) -> ControlArea {
        debug!("process_detail_event: {:?}", event);
        let (mode, control_area) = self.mode.process_event(event);
        if self.mode != mode {
            self.mode = mode;
            self.process_mode_change();
        }

        control_area
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
    }

    fn toggle_tab(&mut self) -> ControlArea {
        self.mode = match self.mode {
            Mode::LaunchControl(_) => Mode::Observables(ObservablesState::Start),
            Mode::Observables(_) => Mode::LaunchControl(LaunchControlState::Start),
        };
        ControlArea::Tabs
    }
}
