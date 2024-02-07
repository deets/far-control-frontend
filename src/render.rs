use std::f64::consts::TAU;

use egui::plot::{Line, LineStyle, Plot, PlotPoints};
use egui::{Color32, Rect, Ui, Vec2};

use crate::state::{ActiveTab, Model};

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

fn render_header(ui: &mut Ui, state: &Model) {
    let mut active_panel = state.active.clone();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut active_panel, ActiveTab::Observables, "Observables");
        ui.selectable_value(
            &mut active_panel,
            ActiveTab::LaunchControl,
            "Launch Control",
        );
    });
    // ui.sel
    // let desired_size = [
    //     ui.available_width(),
    //     ui.available_height() * layout::header::MARGIN,
    // ];
    // let (_id, rect) = ui.allocate_space(desired_size.into());
    // ui.painter().rect(rect, 0.0, Color32::RED, Stroke::NONE);
    // let (lr, rr) = split_rect_horizontally_at(&rect, 0.5);
    // let (active, background, active_rect) = match state.active {
    //     ActiveTab::Observables => (
    //         layout::colors::OBSERVABLES,
    //         muted(layout::colors::LAUNCHCONTROL),
    //         lr,
    //     ),
    //     ActiveTab::LaunchControl => (
    //         layout::colors::LAUNCHCONTROL,
    //         muted(layout::colors::OBSERVABLES),
    //         rr,
    //     ),
    // };
    // let active = match state.control {
    //     ControlArea::Details => muted(active),
    //     ControlArea::Tabs => active,
    // };
    // ui.painter().rect(rect, 0.0, background, Stroke::NONE);
    //ui.painter().rect(active_rect, 0.0, active, Stroke::NONE);
}

fn render_launch_control(ui: &mut Ui, model: &Model) {
    ui.vertical_centered(|ui| {
        ui.label(match model.state() {
            crate::state::State::Start => "Start",
            crate::state::State::Failure => "Failure",
            crate::state::State::Reset => "Reset",
            crate::state::State::Idle => "Idle",
        });
    });
}

fn render_body(ui: &mut Ui, state: &Model) {
    match state.active {
        ActiveTab::Observables => {}
        ActiveTab::LaunchControl => {
            render_launch_control(ui, state);
        }
    }
}

fn render_status(ui: &mut Ui, state: &Model) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.ctx().request_repaint();

        let elapsed = state.elapsed();
        let mut plot = Plot::new("lines_demo")
            .height(ui.available_height())
            .width(ui.available_height())
            .show_axes([false, false])
            .show_background(false);
        plot = plot.data_aspect(1.0);
        plot.show(ui, |ui| {
            let steps = 16;
            ui.line(
                Line::new(PlotPoints::from_explicit_callback(
                    move |x| 0.5 * (TAU * (x + elapsed.as_secs_f64())).sin(),
                    0.0..=1.0,
                    steps,
                ))
                .color(Color32::from_rgb(200, 100, 100))
                .style(LineStyle::Solid)
                .name("wave"),
            );
        });
    });
}

pub fn render(ui: &mut Ui, state: &Model) {
    egui::TopBottomPanel::top("top_panel")
        .resizable(false)
        .min_height(ui.spacing().interact_size.y * 2.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                render_header(ui, state);
            });
        });
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(false)
        .min_height(ui.spacing().interact_size.y * 2.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                render_status(ui, state);
            });
        });
    egui::CentralPanel::default().show_inside(ui, |ui| {
        render_body(ui, state);
    });
}
