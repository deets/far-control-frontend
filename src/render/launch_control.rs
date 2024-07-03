use egui::{RichText, Sense, Ui};
use emath::Align2;
use epaint::{Color32, FontId, Shadow};

use crate::{
    layout::colors::{kind_color32, Intensity, Kind},
    model::LaunchControlMode,
    observables::rqb::ObservablesGroup2,
};

use super::{clear_frame, render_progress, rq_render::render_pyro_state, text_color};

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

pub fn render_launch_control(
    ui: &mut Ui,
    state: &LaunchControlMode,
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
