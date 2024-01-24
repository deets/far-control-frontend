use crate::input::InputEvent;

pub enum ActiveTab {
    Observables,
    LaunchControl,
}

pub enum ControlArea {
    Tabs,
    Details,
}

pub struct State {
    pub active: ActiveTab,
    pub control: ControlArea,
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

impl Default for State {
    fn default() -> Self {
        Self {
            active: Default::default(),
            control: Default::default(),
        }
    }
}

impl State {
    pub fn process_input_events(&mut self, events: &Vec<InputEvent>) {
        for event in events {
            self.process_input_event(event);
        }
    }

    fn process_input_event(&mut self, event: &InputEvent) {
        println!("process input event: {:?}", event);
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
