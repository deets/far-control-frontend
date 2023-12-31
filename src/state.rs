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
