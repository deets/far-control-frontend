use std::time::{Duration, Instant};

use crate::{consort::Consort, input::InputEvent};

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
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    pub fn update_time(&mut self, now: Instant) {
        self.now = now;
        self.consort.update_time(now);
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
