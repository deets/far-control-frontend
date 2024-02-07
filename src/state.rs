use anyhow::anyhow;

use log::{debug, error, warn};
#[cfg(test)]
use mock_instant::Instant;
use ringbuffer::{AllocRingBuffer, RingBuffer};
#[cfg(not(test))]
use std::time::Instant;

use std::{cell::RefCell, io::Write, rc::Rc, time::Duration};

use crate::{
    consort::Consort, ebyte::E32Connection, input::InputEvent, rqparser::MAX_BUFFER_SIZE,
    rqprotocol::Command,
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

pub struct State<'a> {
    pub active: ActiveTab,
    pub control: ControlArea,
    pub consort: Consort<'a>,
    start: Instant,
    now: Instant,
    send: bool,
}

impl Default for ActiveTab {
    fn default() -> Self {
        Self::Observables
    }
}

impl Default for ControlArea {
    fn default() -> Self {
        Self::Tabs
    }
}

impl<'a> State<'a> {
    pub fn new(consort: Consort<'a>, now: Instant) -> Self {
        Self {
            active: Default::default(),
            control: Default::default(),
            consort,
            start: now,
            now,
            send: false,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    pub fn drive(&mut self, now: Instant, module: &mut E32Connection) -> anyhow::Result<()> {
        self.now = now;
        self.consort.update_time(now);
        let mut ringbuffer = AllocRingBuffer::new(MAX_BUFFER_SIZE);
        module.recv(|answer| match answer {
            crate::ebyte::Answers::Received(sentence) => {
                debug!("got sentence {:?}", std::str::from_utf8(&sentence).unwrap());
                for c in sentence {
                    ringbuffer.push(c);
                }
            }
            crate::ebyte::Answers::Timeout => {
                error!("Timeout, we need to reset!");
            }
        });
        if ringbuffer.len() > 0 {
            debug!("rb len before feeding: {}", ringbuffer.len());
        }
        while !ringbuffer.is_empty() {
            match self.consort.feed(&mut ringbuffer) {
                Ok(_) => {
                    debug!("consort happy, rb len: {}", ringbuffer.len());
                    self.consort.reset();
                }
                Err(err) => {
                    error!("consort unhappy: {:?}", err);
                    self.consort.reset();
                    break;
                }
            }
        }

        if self.send && !self.consort.busy() {
            debug!("triggering send via key");
            self.send = false;
            match self.consort.send_command(Command::Reset, module) {
                Ok(_) => {
                    debug!("sent data");
                }
                Err(_) => {
                    return Err(anyhow!("Command::Reset error"));
                }
            };
        }
        Ok(())
    }

    pub fn process_input_events(&mut self, events: &Vec<InputEvent>) {
        for event in events {
            match event {
                InputEvent::Send => {
                    self.send = true;
                }
                _ => {}
            }
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
