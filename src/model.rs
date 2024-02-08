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
    start: Instant,
    now: Instant,
}

pub trait StateProcessing {
    type State;

    fn process_response(&self, response: Response) -> Self::State;

    fn name(&self) -> &str;

    fn is_failure(&self) -> bool;
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

impl<'a> Model<'a> {
    pub fn new(consort: Consort<'a>, now: Instant) -> Self {
        Self {
            mode: Default::default(),
            control: Default::default(),
            consort,
            start: now,
            now,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn hi_secret_a(&self) -> u8 {
        0
    }

    pub fn lo_secret_a(&self) -> u8 {
        1
    }

    pub fn hi_secret_b(&self) -> u8 {
        2
    }

    pub fn lo_secret_b(&self) -> u8 {
        11
    }

    pub fn drive(&mut self, now: Instant, module: &mut E32Connection) -> anyhow::Result<()> {
        self.now = now;
        self.consort.update_time(now);
        // When we are in start state, start a reset cycle
        match self.mode {
            Mode::Observables(ObservablesState::Start) => {
                debug!("Resetting because we are in Start");
                self.reset(module);
                return Ok(());
            }
            Mode::LaunchControl(LaunchControlState::Start) => {
                debug!("Resetting because we are in Start");
                self.reset(module);
                return Ok(());
            }
            _ => {}
        }

        let mut ringbuffer = AllocRingBuffer::new(MAX_BUFFER_SIZE);
        let mut timeout = false;
        module.recv(|answer| match answer {
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
            self.reset(module);
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
                        self.reset(module);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn reset(&mut self, module: &mut E32Connection) {
        self.mode = match self.mode {
            Mode::Observables(_) => Mode::Observables(ObservablesState::Reset),
            Mode::LaunchControl(_) => Mode::LaunchControl(LaunchControlState::Reset),
        };
        self.consort.reset();
        match self.consort.send_command(Command::Reset, module) {
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
        match self.control {
            ControlArea::Tabs => self.process_tabs_event(event),
            ControlArea::Details => self.process_details_event(event),
        }
    }

    fn process_tabs_event(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Left(..) => self.toggle_tab(),
            InputEvent::Right(..) => self.toggle_tab(),
            InputEvent::Enter => self.enter(),
            _ => {}
        }
    }

    fn process_details_event(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Back => self.exit(),
            _ => {}
        }
    }

    fn toggle_tab(&mut self) {
        self.mode = match self.mode {
            Mode::LaunchControl(_) => Mode::Observables(ObservablesState::Start),
            Mode::Observables(_) => Mode::LaunchControl(LaunchControlState::Start),
        }
    }

    fn enter(&mut self) {
        self.control = ControlArea::Details;
    }

    fn exit(&mut self) {
        self.control = ControlArea::Tabs;
    }
}
