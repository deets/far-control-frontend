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
pub enum ActiveTab {
    Observables,
    LaunchControl,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlArea {
    Tabs,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Start,
    Failure,
    Reset,
    Idle,
}

pub struct Model<'a> {
    pub active: ActiveTab,
    pub control: ControlArea,
    pub consort: Consort<'a>,
    start: Instant,
    now: Instant,
    state: State,
}

impl Default for ActiveTab {
    fn default() -> Self {
        Self::LaunchControl
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
            active: Default::default(),
            control: Default::default(),
            consort,
            start: now,
            now,
            state: State::Start,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn drive(&mut self, now: Instant, module: &mut E32Connection) -> anyhow::Result<()> {
        self.now = now;
        self.consort.update_time(now);
        // When we are in start state, start a reset cycle
        match self.state {
            State::Start => {
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
        self.state = State::Reset;
        self.consort.reset();
        match self.consort.send_command(Command::Reset, module) {
            Ok(_) => {}
            Err(_) => {
                error!("Resetting failed");
                self.state = State::Failure;
            }
        }
    }

    fn process_response(&mut self, response: Response) {
        match self.state {
            State::Start => {}
            State::Failure => todo!(),
            State::Reset => match response {
                Response::ResetAck => {
                    debug!("Acknowledged Reset, go to Idle");
                    self.state = State::Idle;
                }
                _ => {
                    self.state = State::Start;
                }
            },
            State::Idle => {}
        }
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
        self.active = match self.active {
            ActiveTab::LaunchControl => ActiveTab::Observables,
            ActiveTab::Observables => ActiveTab::LaunchControl,
        }
    }

    fn enter(&mut self) {
        self.control = ControlArea::Details;
    }

    fn exit(&mut self) {
        self.control = ControlArea::Tabs;
    }
}
