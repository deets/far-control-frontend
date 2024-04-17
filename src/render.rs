use std::time::Duration;

use egui::epaint::Shadow;
use egui::{vec2, Align2, Color32, FontId, Frame, Id, ProgressBar, RichText, Sense, Stroke, Ui};
use emath::{pos2, Pos2, Vec2};
use palette::{Gradient, LinSrgb};
use uom::si::f64::Mass;

use crate::connection::Connection;
use crate::layout::colors::{color32, kind_color, kind_color32, Intensity, Kind};
use crate::model::{ControlArea, LaunchControlState, Mode, Model, StateProcessing};
use crate::observables::rqa::{ObservablesGroup1, ObservablesGroup2, PyroStatus, RecordingState};

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

fn render_header_text(ui: &mut Ui, text: &str, color: Color32) {
    let digit_font = FontId::new(32.0, egui::FontFamily::Monospace);
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(text.into(), digit_font.clone(), color);
    let rect = galley.size();
    let (response, painter) = ui.allocate_painter(rect.into(), Sense::hover());
    painter.text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        text,
        digit_font,
        color,
    );
}

fn intensity(selected: bool) -> Intensity {
    if selected {
        Intensity::High
    } else {
        Intensity::Low
    }
}

fn text_color(active: bool) -> Color32 {
    if active {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

fn kind_for_mode(mode: &Mode) -> Kind {
    match mode {
        Mode::Observables(_) => Kind::Observables,
        Mode::LaunchControl(_) => Kind::LaunchControl,
    }
}

fn render_header<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    let is_observables = match model.mode() {
        Mode::Observables(_) => true,
        Mode::LaunchControl(_) => false,
    };
    let is_tabs = match model.control {
        ControlArea::Tabs => true,
        ControlArea::Details => false,
    };

    ui.horizontal(|ui| {
        egui::SidePanel::left("observables")
            .resizable(false)
            .show_separator_line(false)
            .frame(color_frame(
                kind_color32(Kind::Observables, intensity(is_observables && is_tabs)),
                10.0,
            ))
            .exact_width(ui.available_width() / 2.0)
            .show_inside(ui, |ui| {
                render_header_text(ui, "Observables", text_color(is_observables && is_tabs));
            });
        egui::SidePanel::right("launch control")
            .resizable(false)
            .show_separator_line(false)
            .frame(color_frame(
                kind_color32(Kind::LaunchControl, intensity(!is_observables && is_tabs)),
                10.0,
            ))
            .exact_width(ui.available_width())
            .show_inside(ui, |ui| {
                render_header_text(ui, "Launch Control", text_color(!is_observables && is_tabs));
            });
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
        text_color(active),
    );
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
        text_color(match state {
            LaunchControlState::WaitForFire { .. } => true,
            _ => false,
        }),
    );
}

fn render_progress(ui: &mut Ui, state: &LaunchControlState, progress: f32, ignition: bool) {
    let gradient = Gradient::new(vec![
        LinSrgb::new(0.0, 1.0, 0.0),
        LinSrgb::new(1.0, 1.0, 0.0),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);
    let color = color32(gradient.get(progress));

    let pbar = ProgressBar::new(progress).fill(match state {
        LaunchControlState::PrepareIgnition { .. } => {
            if ignition {
                color
            } else {
                Color32::DARK_GRAY
            }
        }
        LaunchControlState::PrepareUnlockPyros { .. } => {
            if !ignition {
                color
            } else {
                Color32::DARK_GRAY
            }
        }
        _ => Color32::DARK_GRAY,
    });
    ui.add(pbar);
}

fn render_launch_control_interactions(ui: &mut Ui, state: &LaunchControlState) {
    let (hi_a, lo_a, hi_b, lo_b) = state.digits();
    let (hi_a_hl, lo_a_hl, hi_b_hl, lo_b_hl) = state.highlights();

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            egui::SidePanel::left("key a left")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(
                        RichText::new("Enter Key A")
                            .color(text_color(hi_a_hl || lo_a_hl))
                            .heading(),
                    );
                });
            render_digit(ui, hi_a, hi_a_hl);
            render_digit(ui, lo_a, lo_a_hl);
        });
        render_progress(ui, state, state.unlock_pyros_progress(), false);
        ui.horizontal(|ui| {
            egui::SidePanel::left("key b left")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(
                        RichText::new("Enter Key B")
                            .color(text_color(hi_b_hl || lo_b_hl))
                            .heading(),
                    );
                });
            render_digit(ui, hi_b, hi_b_hl);
            render_digit(ui, lo_b, lo_b_hl);
        });
        render_progress(ui, state, state.prepare_ignition_progress(), true);
        render_fire(ui, state);
    });
}

fn vbb_from_obg2(obg2: &Option<ObservablesGroup2>) -> String {
    match obg2 {
        Some(obg2) => format!("{:03.2}", obg2.vbb_voltage),
        None => "--.--".into(),
    }
}

fn render_pyro_state(ui: &mut Ui, pyro_status: Option<PyroStatus>, height: f32) {
    let rect = Vec2::new(ui.available_width(), height);
    let (_response, painter) = ui.allocate_painter(rect.into(), Sense::hover());
    let center = painter.clip_rect().center();
    painter.circle_filled(center, height * 1.0 * 0.5, Color32::BLACK);
    painter.circle_filled(
        center,
        height * 0.9 * 0.5,
        match pyro_status {
            Some(pyro_status) => match pyro_status {
                PyroStatus::Unknown => Color32::DARK_GRAY,
                PyroStatus::Open => Color32::RED,
                PyroStatus::Closed => Color32::GREEN,
            },
            None => Color32::DARK_GRAY,
        },
    );
}

fn render_launch_control_powerstate(ui: &mut Ui, obg2: &Option<ObservablesGroup2>) {
    let digit_font = FontId::new(54.0, egui::FontFamily::Monospace);
    let painter = ui.painter();
    let galley = painter.layout_no_wrap("X".into(), digit_font.clone(), Color32::RED);
    let char_height = galley.rect.height();

    ui.vertical(|ui| {
        ui.label(
            RichText::new("VBB")
                .font(digit_font.clone())
                .color(Color32::BLACK),
        );
        ui.label(
            RichText::new(vbb_from_obg2(obg2))
                .font(digit_font.clone())
                .color(Color32::BLACK),
        );
        ui.label(
            RichText::new("Pyro 1/2")
                .font(digit_font.clone())
                .color(Color32::BLACK),
        );
        render_pyro_state(
            ui,
            obg2.clone().and_then(|obg2| Some(obg2.pyro12_status)),
            char_height,
        );
        ui.label(
            RichText::new("Pyro 3/4")
                .font(digit_font.clone())
                .color(Color32::BLACK),
        );
        render_pyro_state(
            ui,
            obg2.clone().and_then(|obg2| Some(obg2.pyro34_status)),
            char_height,
        );
    });
}

fn render_launch_control(
    ui: &mut Ui,
    state: &LaunchControlState,
    obg2: &Option<ObservablesGroup2>,
) {
    ui.horizontal(|ui| {
        let left_width = (ui.available_width() * 0.7).ceil();
        let right_width = ui.available_width() - left_width;
        egui::SidePanel::left("lc_interactions")
            .resizable(false)
            .show_separator_line(false)
            .frame(clear_frame())
            .exact_width(left_width)
            .show_inside(ui, |ui| render_launch_control_interactions(ui, state));
        egui::SidePanel::right("powerstate")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::containers::Frame {
                rounding: egui::Rounding::default(),
                fill: kind_color32(Kind::Observables, Intensity::Low),
                stroke: egui::Stroke::NONE,
                inner_margin: 10.0.into(),
                outer_margin: 10.0.into(),
                shadow: Shadow::NONE,
            })
            .exact_width(right_width)
            .show_inside(ui, |ui| render_launch_control_powerstate(ui, obg2));
    });
}

fn render_uptime(ui: &mut Ui, uptime: Duration) {
    let secs = uptime.as_secs_f64();
    ui.label(
        RichText::new(format!("{}", secs))
            .color(text_color(false))
            .heading(),
    );
}

fn render_thrust(ui: &mut Ui, thrust: Mass) {
    ui.label(
        RichText::new(format!("{:?}", thrust))
            .color(text_color(false))
            .heading(),
    );
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
                    ui.label(
                        RichText::new("Timestamp")
                            .color(text_color(false))
                            .heading(),
                    );
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
                    ui.label(RichText::new("Thrust").color(text_color(false)).heading());
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
                    ui.label(
                        RichText::new("Recording State")
                            .color(text_color(false))
                            .heading(),
                    );
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
                    ui.label(
                        RichText::new("Anomalies")
                            .color(text_color(false))
                            .heading(),
                    );
                });
            if let Some(obg2) = obg2 {
                ui.label(
                    RichText::new(format!("{}", obg2.anomalies))
                        .heading()
                        .color(Color32::WHITE),
                );
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("vbb_voltage")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(
                        RichText::new("VBB Voltage")
                            .color(text_color(false))
                            .heading(),
                    );
                });
            if let Some(obg2) = obg2 {
                ui.label(
                    RichText::new(format!("{}", obg2.vbb_voltage))
                        .heading()
                        .color(Color32::WHITE),
                );
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("pyro_status")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Pyros").color(text_color(false)).heading());
                });
            if let Some(obg2) = obg2 {
                ui.label(
                    RichText::new(format!("1/2: {:?}", obg2.pyro12_status))
                        .heading()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new(format!("3/4: {:?}", obg2.pyro34_status))
                        .heading()
                        .color(Color32::WHITE),
                );
            }
        });
    });
}

fn render_body<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, state: &Model<C, Id>) {
    let obg2 = state.obg2.clone();
    match state.mode {
        Mode::Observables(_state) => render_observables(ui, &state.obg1, &obg2),
        Mode::LaunchControl(state) => {
            render_launch_control(ui, &state, &obg2);
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

fn clear_frame() -> Frame {
    egui::containers::Frame {
        rounding: egui::Rounding::default(),
        fill: Color32::TRANSPARENT,
        stroke: egui::Stroke::NONE,
        inner_margin: 0.0.into(),
        outer_margin: 0.0.into(),
        shadow: Shadow::NONE,
    }
}

fn color_frame(color: Color32, padding: f32) -> Frame {
    egui::containers::Frame {
        rounding: egui::Rounding::default(),
        fill: color,
        stroke: egui::Stroke::NONE,
        inner_margin: padding.into(),
        outer_margin: 0.0.into(),
        shadow: Shadow::NONE,
    }
}

fn status_background_frame<C: Connection, IdGenerator: Iterator<Item = usize>>(
    ui: &mut Ui,
    model: &Model<C, IdGenerator>,
) -> Frame {
    let id = Id::new("status_background_frame");
    let how_connected = ui.ctx().animate_bool_with_time(id, !model.connected(), 0.5);

    let gradient = Gradient::new(vec![
        kind_color(Kind::Status, Intensity::Low),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);

    let fill = color32(gradient.get(how_connected));

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
                left: 0.,
                right: 0.,
                top: 0.,
                bottom: 0.,
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
        .frame(clear_frame())
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
        .frame(color_frame(
            kind_color32(kind_for_mode(model.mode()), intensity(!tabs_active)),
            10.0,
        ))
        .show_inside(ui, |ui| {
            render_body(ui, model);
        });
}
