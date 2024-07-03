use std::time::Duration;

use egui::epaint::Shadow;
use egui::{vec2, Align2, Color32, FontId, Frame, Id, ProgressBar, RichText, Sense, Stroke, Ui};
use emath::{pos2, Pos2};
use palette::{Gradient, LinSrgb};

use crate::connection::Connection;
use crate::ebyte::modem_baud_rate;
use crate::layout::colors::{color32, kind_color, kind_color32, Intensity, Kind};
use crate::model::{ControlArea, LaunchControlMode, Mode, Model, RFSilenceMode, StateProcessing};
use crate::observables::AdcGain;

#[cfg(feature = "test-stand")]
use crate::observables::rqa as rqobs;

#[cfg(feature = "rocket")]
use crate::observables::rqb as rqobs;

use rqobs::ObservablesGroup2;

#[cfg(feature = "test-stand")]
pub mod rqa;
#[cfg(feature = "rocket")]
pub mod rqb;

#[cfg(feature = "test-stand")]
use self::rqa as rq_render;
#[cfg(feature = "rocket")]
use self::rqb as rq_render;

use self::rq_render::{render_observables, render_pyro_state};

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
        Mode::RFSilence(_) => Kind::RFSilence,
    }
}

fn render_header<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    let is_observables = match model.mode() {
        Mode::Observables(_) => true,
        _ => false,
    };
    let is_launch_control = match model.mode() {
        Mode::LaunchControl(_) => true,
        _ => false,
    };
    let is_rf_silence = match model.mode() {
        Mode::RFSilence(_) => true,
        _ => false,
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
            .exact_width(ui.available_width() / 3.0)
            .show_inside(ui, |ui| {
                render_header_text(ui, "Observables", text_color(is_observables && is_tabs));
            });
        egui::SidePanel::left("launch control")
            .resizable(false)
            .show_separator_line(false)
            .frame(color_frame(
                kind_color32(Kind::LaunchControl, intensity(is_launch_control && is_tabs)),
                10.0,
            ))
            .exact_width(ui.available_width() / 2.0)
            .show_inside(ui, |ui| {
                render_header_text(
                    ui,
                    "Launch Control",
                    text_color(is_launch_control && is_tabs),
                );
            });
        egui::SidePanel::left("RF silence")
            .resizable(false)
            .show_separator_line(false)
            .frame(color_frame(
                kind_color32(Kind::LaunchControl, intensity(is_rf_silence && is_tabs)),
                10.0,
            ))
            .exact_width(ui.available_width())
            .show_inside(ui, |ui| {
                render_header_text(ui, "RF Silence", text_color(is_rf_silence && is_tabs));
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

fn render_fire(ui: &mut Ui, state: &LaunchControlMode) {
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
            LaunchControlMode::WaitForFire { .. } => true,
            _ => false,
        }),
    );
}

fn render_progress(ui: &mut Ui, state: &LaunchControlMode, progress: f32, ignition: bool) {
    let gradient = Gradient::new(vec![
        LinSrgb::new(0.0, 1.0, 0.0),
        LinSrgb::new(1.0, 1.0, 0.0),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);
    let color = color32(gradient.get(progress));

    let pbar = ProgressBar::new(progress).fill(match state {
        LaunchControlMode::PrepareIgnition { .. } => {
            if ignition {
                color
            } else {
                Color32::DARK_GRAY
            }
        }
        LaunchControlMode::PrepareUnlockPyros { .. } => {
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

fn render_launch_control_interactions(ui: &mut Ui, state: &LaunchControlMode) {
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
        ui.label(
            RichText::new("Unlock Pyros")
                .color(text_color(
                    if let LaunchControlMode::PrepareUnlockPyros { .. } = state {
                        true
                    } else {
                        false
                    },
                ))
                .heading(),
        );
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
        ui.label(
            RichText::new("Arm Pyros")
                .color(text_color(
                    if let LaunchControlMode::PrepareIgnition { .. } = state {
                        true
                    } else {
                        false
                    },
                ))
                .heading(),
        );
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

fn render_rocket_screen(ui: &mut Ui) {
    let giant_font = FontId::new(250.0, egui::FontFamily::Monospace);
    let color = Color32::WHITE;
    let painter = ui.painter();
    let galley = painter.layout_no_wrap("🚀".into(), giant_font.clone(), color);
    let rect = galley.size();
    let (response, painter) = ui.allocate_painter(rect.into(), Sense::hover());
    painter.text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        "🚀",
        giant_font,
        color,
    );
}

fn render_launch_control(ui: &mut Ui, state: &LaunchControlMode, obg2: &Option<ObservablesGroup2>) {
    ui.horizontal(|ui| {
        let left_width = (ui.available_width() * 0.7).ceil();
        let right_width = ui.available_width() - left_width;
        egui::SidePanel::left("lc_interactions")
            .resizable(false)
            .show_separator_line(false)
            .frame(clear_frame())
            .exact_width(left_width)
            .show_inside(ui, |ui| match state {
                LaunchControlMode::WaitForPyroTimeout(_) => render_rocket_screen(ui),
                LaunchControlMode::SwitchToObservables => render_rocket_screen(ui),
                _ => {
                    render_launch_control_interactions(ui, state);
                }
            });
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

fn render_rf_silence(ui: &mut Ui, state: RFSilenceMode) {}

fn render_body<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, state: &Model<C, Id>) {
    let obg2 = state.obg2.clone();
    match state.mode {
        Mode::Observables(_state) => render_observables(ui, state),
        Mode::LaunchControl(state) => {
            render_launch_control(ui, &state, &obg2);
        }
        Mode::RFSilence(state) => {
            render_rf_silence(ui, state);
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

fn render_nrf_state(ui: &mut Ui, heard_of_since: Duration) {
    let gradient = Gradient::new(vec![
        LinSrgb::new(0.0, 1.0, 0.0),
        LinSrgb::new(1.0, 1.0, 0.0),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);
    let progress = match heard_of_since.as_secs() {
        0..10 => heard_of_since.as_secs_f32() / 10.0,
        _ => 1.0,
    };

    let color = color32(gradient.get(progress));
    let rect = ui.spacing().interact_size;
    let (_response, painter) = ui.allocate_painter(rect.into(), Sense::hover());
    let center = painter.clip_rect().center();
    painter.circle_filled(center, rect.y * 1.0 * 0.5, Color32::BLACK);
    painter.circle_filled(center, rect.y * 0.8 * 0.5, color);
}

fn render_status<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
    ui.horizontal(|ui| {
        if model.mode.core_mode().is_failure() {
            ui.spinner();
        } else {
            render_alive(ui);
        };
        ui.label(model.mode().name());
        ui.label(format!("E32 baud rate: {:?}", modem_baud_rate()));
        ui.label(format!(
            "Gain: {:?}",
            match model.adc_gain {
                AdcGain::Gain1 => 1,
                AdcGain::Gain2 => 2,
                AdcGain::Gain4 => 4,
                AdcGain::Gain8 => 8,
                AdcGain::Gain16 => 16,
                AdcGain::Gain32 => 32,
                AdcGain::Gain64 => 64,
            }
        ));
        ui.label(format!(
            "Connected: {}",
            model.uptime().map_or("--:--".to_string(), |duration| {
                let seconds = duration.as_secs();
                format!("{}:{:02}", seconds / 60, seconds % 60)
            })
        ));
        ui.label(
            model
                .recorder_path
                .clone()
                .map_or("Not recording to file".to_string(), |path| {
                    format!("Recording: {:?}", path)
                }),
        );
        if let Some(reset_countdown) = model.auto_reset_in() {
            ui.label(format!("Automatic reset in: {}", reset_countdown.as_secs()));
        }
        for node in model.registered_nodes() {
            let heard_of_since = model.heard_from_since(&node);
            let name = match node {
                crate::rqprotocol::Node::RedQueen(id) => {
                    let buf = [b'R', b'Q', id];
                    unsafe { std::str::from_utf8_unchecked(&buf) }.to_string()
                }
                crate::rqprotocol::Node::Farduino(id) => {
                    let buf = [b'F', b'D', id];
                    unsafe { std::str::from_utf8_unchecked(&buf) }.to_string()
                }
                crate::rqprotocol::Node::LaunchControl => "LNC".to_string(),
            };
            ui.label(name);
            render_nrf_state(ui, heard_of_since);
        }
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
