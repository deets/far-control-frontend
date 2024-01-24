use egui::{Color32, Rect, Stroke, Ui, Vec2};

use crate::layout;
use crate::layout::colors::muted;
use crate::state::{ActiveTab, ControlArea, State};

fn split_rect_horizontally_at(rect: &Rect, split: f32) -> (Rect, Rect) {
    let lt = rect.left_top();
    let h = rect.height();
    let left_width = rect.width() * split;
    let right_width = rect.width() - left_width;
    let mt = lt + Vec2::new(left_width, 0.0);
    let left = Rect::from_min_size(lt, [left_width, h].into());
    let right = Rect::from_min_size(mt, [right_width, h].into());
    (left, right)
}

fn render_header(ui: &mut Ui, state: &State) {
    let desired_size = [
        ui.available_width(),
        ui.available_height() * layout::header::MARGIN,
    ];
    let (_id, rect) = ui.allocate_space(desired_size.into());
    ui.painter().rect(rect, 0.0, Color32::RED, Stroke::NONE);
    let (lr, rr) = split_rect_horizontally_at(&rect, 0.5);
    let (active, background, active_rect) = match state.active {
        ActiveTab::Observables => (
            layout::colors::OBSERVABLES,
            muted(layout::colors::LAUNCHCONTROL),
            lr,
        ),
        ActiveTab::LaunchControl => (
            layout::colors::LAUNCHCONTROL,
            muted(layout::colors::OBSERVABLES),
            rr,
        ),
    };
    let active = match state.control {
        ControlArea::Details => muted(active),
        ControlArea::Tabs => active,
    };
    ui.painter().rect(rect, 0.0, background, Stroke::NONE);
    ui.painter().rect(active_rect, 0.0, active, Stroke::NONE);
}

fn render_body(ui: &mut Ui, state: &State) {
    let desired_size = [ui.available_width(), ui.available_height()];
    let (_id, rect) = ui.allocate_space(desired_size.into());
    let color = match state.active {
        ActiveTab::Observables => layout::colors::OBSERVABLES,
        ActiveTab::LaunchControl => layout::colors::LAUNCHCONTROL,
    };
    let color = match state.control {
        ControlArea::Tabs => muted(color),
        ControlArea::Details => color,
    };
    ui.painter().rect(rect, 0.0, color, Stroke::NONE);
}

pub fn render(ui: &mut Ui, state: &State) {
    render_header(ui, state);
    render_body(ui, state);
}
