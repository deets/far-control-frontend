use std::f64::consts::TAU;

use egui::epaint::Shadow;
use egui::plot::{Line, LineStyle, Plot, PlotPoints};
use egui::{Align2, Color32, FontId, Frame, Rect, Rounding, Sense, Stroke, Ui, Vec2};

use crate::model::{ActiveTab, ControlArea, Model, State};

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
}

fn render_digit(ui: &mut Ui, digit: u8, active: bool) {
    let digit_font = FontId::new(54.0, egui::FontFamily::Monospace);
    let painter = ui.painter();
    let text = match digit {
        0..10 => format!("{}", digit),
        10..16 => format!("{}", std::str::from_utf8(&[55 + digit]).expect("")),
        _ => unreachable!(),
    };

    let galley = painter.layout_no_wrap(text.clone(), digit_font.clone(), Color32::RED);
    let rect = galley.size();
    let (response, painter) = ui.allocate_painter(rect.into(), Sense::hover());

    painter.text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        text,
        digit_font,
        if active {
            Color32::WHITE
        } else {
            Color32::DARK_GRAY
        },
    );
    painter.rect(
        response.rect,
        Rounding::default(),
        Color32::TRANSPARENT,
        Stroke::new(4.0, Color32::RED),
    );
}

fn render_launch_control(ui: &mut Ui, model: &Model) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            render_digit(ui, model.hi_secret_a(), true);
            render_digit(ui, model.lo_secret_a(), false);
        });
        ui.horizontal(|ui| {
            render_digit(ui, model.hi_secret_b(), false);
            render_digit(ui, model.lo_secret_b(), false);
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

fn render_status(ui: &mut Ui, model: &Model) {
    ui.horizontal(|ui| {
        match model.state() {
            crate::model::State::Failure => {
                ui.spinner();
            }
            _ => {
                ui.ctx().request_repaint();

                let elapsed = model.elapsed();
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
            }
        };
        ui.label(match model.state() {
            State::Start => "Start",
            State::Failure => "Failure",
            State::Reset => "Reset",
            State::Idle => "Idle",
        });
    });
}

fn frame(active: bool) -> Frame {
    egui::containers::Frame {
        rounding: egui::Rounding {
            nw: 1.0,
            ne: 1.0,
            sw: 1.0,
            se: 1.0,
        },
        fill: match active {
            true => Color32::DARK_RED,
            false => Color32::TRANSPARENT,
        },
        stroke: egui::Stroke::NONE,
        inner_margin: {
            egui::style::Margin {
                left: 10.,
                right: 10.,
                top: 10.,
                bottom: 10.,
            }
        },
        outer_margin: {
            egui::style::Margin {
                left: 10.,
                right: 10.,
                top: 10.,
                bottom: 10.,
            }
        },
        shadow: Shadow::NONE,
    }
}

pub fn render(ui: &mut Ui, model: &Model) {
    let tabs_active = match model.control {
        ControlArea::Tabs => true,
        ControlArea::Details => false,
    };
    egui::TopBottomPanel::top("top_panel")
        .resizable(false)
        .frame(frame(tabs_active))
        .min_height(ui.spacing().interact_size.y * 2.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                render_header(ui, model);
            });
        });
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(false)
        .min_height(ui.spacing().interact_size.y * 2.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                render_status(ui, model);
            });
        });
    egui::CentralPanel::default()
        .frame(frame(!tabs_active))
        .show_inside(ui, |ui| {
            render_body(ui, model);
        });
}
