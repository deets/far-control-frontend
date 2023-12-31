use imgui::Ui;

use crate::state::State;

fn render_header(ui: &Ui, state: &State) {
    ui.text("Observables");
    ui.same_line();
    ui.text("Launch Control");
}

pub fn render(ui: &Ui, state: &State) {
    render_header(ui, state);
}
