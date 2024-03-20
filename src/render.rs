use std::time::Duration;

use egui::epaint::Shadow;
use egui::{vec2, Align2, Color32, FontId, Frame, Id, ProgressBar, RichText, Sense, Stroke, Ui};
use emath::{pos2, Pos2};
use palette::{Gradient, LinSrgb};
use uom::si::f64::Mass;

use crate::connection::Connection;
use crate::model::{ControlArea, LaunchControlState, Mode, Model, StateProcessing};
use crate::observables::rqa::{ObservablesGroup1, ObservablesGroup2, RecordingState};

// fn split_rect_horizontally_at(rect: &Rect, split: f32) -> (Rect, Rect) {
//     let lt = rect.left_top();
//     let h = rect.height();
//     let left_width = rect.width() * split;
//     let right_width = rect.width() - left_width;
//     let mt = lt + Vec2::new(left_width, 0.0);
//     let left = Rect::from_min_size(lt, [left_width, h].into());
//     let right = Rect::from_min_size(mt, [right_width, h].into());
//     (left, right)
// }

fn render_header<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    ui.horizontal(|ui| {
        let is_observables = match model.mode() {
            Mode::Observables(_) => true,
            Mode::LaunchControl(_) => false,
        };
        let _ = ui.selectable_label(is_observables, "Observables");
        let _ = ui.selectable_label(!is_observables, "Launch Control");
    });
}

fn active_color(active: bool) -> Color32 {
    if active {
        Color32::WHITE
    } else {
        Color32::DARK_GRAY
    }
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
        active_color(active),
    );
    // painter.rect(
    //     response.rect,
    //     Rounding::default(),
    //     Color32::TRANSPARENT,
    //     Stroke::new(4.0, Color32::RED),
    // );
}

fn render_fire(ui: &mut Ui, state: &LaunchControlState) {
    let digit_font = FontId::new(54.0, egui::FontFamily::Monospace);
    let painter = ui.painter();
    let text = "Press Enter to Fire!";
    let galley = painter.layout_no_wrap(text.into(), digit_font.clone(), Color32::RED);
    let rect = galley.size();
    let (response, painter) = ui.allocate_painter(rect.into(), Sense::hover());

    painter.text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        text,
        digit_font,
        active_color(match state {
            LaunchControlState::WaitForFire { .. } => true,
            _ => false,
        }),
    );
}

fn render_progress(ui: &mut Ui, state: &LaunchControlState) {
    let progress = state.prepare_ignition_progress() as f32 / 100.0;
    let gradient = Gradient::new(vec![
        LinSrgb::new(0.0, 1.0, 0.0),
        LinSrgb::new(1.0, 1.0, 0.0),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);
    let color = gradient.get(progress);
    let color = Color32::from_rgb(
        (color.red * 255.0) as u8,
        (color.green * 255.0) as u8,
        (color.blue * 255.0) as u8,
    );

    let pbar = ProgressBar::new(progress).fill(match state {
        LaunchControlState::PrepareIgnition { .. } => color,
        _ => Color32::DARK_GRAY,
    });
    ui.add(pbar);
}

fn render_launch_control(ui: &mut Ui, state: &LaunchControlState) {
    let (hi_a, lo_a, hi_b, lo_b) = state.digits();
    let (hi_a_hl, lo_a_hl, hi_b_hl, lo_b_hl) = state.highlights();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            egui::SidePanel::left("secret a left")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Enter Secret A").heading());
                });
            render_digit(ui, hi_a, hi_a_hl);
            render_digit(ui, lo_a, lo_a_hl);
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("secret b left")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Enter Secret B").heading());
                });
            render_digit(ui, hi_b, hi_b_hl);
            render_digit(ui, lo_b, lo_b_hl);
        });
        render_progress(ui, state);
        render_fire(ui, state);
    });
}

fn render_uptime(ui: &mut Ui, uptime: Duration) {
    let secs = uptime.as_secs_f64();
    ui.label(RichText::new(format!("{}", secs)).heading());
}

fn render_thrust(ui: &mut Ui, thrust: Mass) {
    ui.label(RichText::new(format!("{:?}", thrust)).heading());
}

fn render_recording_state(ui: &mut Ui, recording_state: &RecordingState) {
    let (text, color) = match &recording_state {
        RecordingState::Unknown => ("Unknown".to_string(), Color32::DARK_GRAY),
        RecordingState::Error(text) => (text.clone(), Color32::RED),
        RecordingState::Pause => ("Pause".to_string(), Color32::DARK_GRAY),
        RecordingState::Recording(filename) => (filename.clone(), Color32::WHITE),
    };
    ui.label(RichText::new(text).heading().color(color));
}

fn render_observables(
    ui: &mut Ui,
    obg1: &Option<ObservablesGroup1>,
    obg2: &Option<ObservablesGroup2>,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            egui::SidePanel::left("timestamp")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Timestamp").heading());
                });
            if let Some(obg1) = obg1 {
                render_uptime(ui, obg1.uptime);
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("thrust")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Thrust").heading());
                });
            if let Some(obg1) = obg1 {
                render_thrust(ui, obg1.thrust);
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("recording")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Recording State").heading());
                });
            if let Some(obg2) = obg2 {
                render_recording_state(ui, &obg2.recording_state);
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("anomalies")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Anomalies").heading());
                });
            if let Some(obg2) = obg2 {
                ui.label(
                    RichText::new(format!("{}", obg2.anomalies))
                        .heading()
                        .color(Color32::WHITE),
                );
            }
        });
    });
}

fn render_body<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, state: &Model<C, Id>) {
    match state.mode {
        Mode::Observables(_state) => render_observables(ui, &state.obg1, &state.obg2),
        Mode::LaunchControl(state) => {
            render_launch_control(ui, &state);
        }
    }
}

fn render_alive(ui: &mut Ui) {
    let color = if ui.visuals().dark_mode {
        Color32::from_additive_luminance(196)
    } else {
        Color32::from_black_alpha(240)
    };

    Frame::canvas(ui.style()).show(ui, |ui| {
        ui.ctx().request_repaint();
        let time = ui.input(|i| i.time);

        let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0);
        let (_id, rect) = ui.allocate_space(desired_size);

        let to_screen = emath::RectTransform::from_to(
            emath::Rect::from_x_y_ranges(0.0..=1.0, -1.0..=1.0),
            rect,
        );

        let mut shapes = vec![];

        for &mode in &[2, 3, 5] {
            let mode = mode as f64;
            let n = 10;
            let speed = 1.5;

            let points: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let t = i as f64 / (n as f64);
                    let amp = (time * speed * mode).sin() / mode;
                    let y = amp * (t * std::f64::consts::TAU / 2.0 * mode).sin();
                    to_screen * pos2(t as f32, y as f32)
                })
                .collect();

            let thickness = 1.0 as f32;
            shapes.push(epaint::Shape::line(points, Stroke::new(thickness, color)));
        }

        ui.painter().extend(shapes);
    });
}

fn render_status<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    ui.horizontal(|ui| {
        if model.mode.is_failure() {
            ui.spinner();
        } else {
            render_alive(ui);
        };
        ui.label(model.mode().name());
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

fn clear_frame() -> Frame {
    egui::containers::Frame {
        rounding: egui::Rounding::default(),
        fill: Color32::TRANSPARENT,
        stroke: egui::Stroke::NONE,
        inner_margin: 1.0.into(),
        outer_margin: 1.0.into(),
        shadow: Shadow::NONE,
    }
}

fn status_background_frame<C: Connection, IdGenerator: Iterator<Item = usize>>(
    ui: &mut Ui,
    model: &Model<C, IdGenerator>,
) -> Frame {
    let id = Id::new("status_background_frame");
    let how_connected = ui.ctx().animate_bool_with_time(id, !model.connected(), 0.5);
    let fill = Color32::DARK_RED.gamma_multiply(how_connected);

    egui::containers::Frame {
        rounding: egui::Rounding {
            nw: 1.0,
            ne: 1.0,
            sw: 1.0,
            se: 1.0,
        },
        fill,
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

pub fn render<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    let tabs_active = match model.control {
        ControlArea::Tabs => true,
        ControlArea::Details => false,
    };
    egui::TopBottomPanel::top("top_panel")
        .resizable(false)
        .show_separator_line(false)
        .frame(frame(tabs_active))
        .min_height(ui.spacing().interact_size.y * 2.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                render_header(ui, model);
            });
        });
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(false)
        .show_separator_line(false)
        .min_height(ui.spacing().interact_size.y * 2.0)
        .frame(status_background_frame(ui, model))
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
