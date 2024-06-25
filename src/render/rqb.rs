use emath::Vec2;
use epaint::Color32;
use std::time::Duration;
use uom::si::{
    f64::{Force, Pressure},
    pressure::bar,
};

use egui::{RichText, Sense, Ui};

use crate::observables::rqb::{ObservablesGroup1, ObservablesGroup2, PyroStatus};

use super::{clear_frame, text_color};

fn render_uptime(ui: &mut Ui, uptime: Duration) {
    let secs = uptime.as_secs_f64();
    ui.label(
        RichText::new(format!("{:.2}", secs))
            .color(text_color(false))
            .heading(),
    );
}

fn render_thrust(ui: &mut Ui, thrust: Force) {
    ui.label(
        RichText::new(format!(
            "{:.8}kN",
            thrust.get::<uom::si::force::kilonewton>()
        ))
        .color(text_color(false))
        .heading(),
    );
}

fn render_pressure(ui: &mut Ui, pressure: Pressure) {
    ui.label(
        RichText::new(format!("{:.6}bar", pressure.get::<bar>()))
            .color(text_color(false))
            .heading(),
    );
}

pub fn render_pyro_state(ui: &mut Ui, pyro_status: Option<PyroStatus>, height: f32) {
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

pub fn render_observables(
    ui: &mut Ui,
    obg1: &Vec<ObservablesGroup1>,
    _obg2: &Option<ObservablesGroup2>,
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
            if let Some(obg1) = obg1.last() {
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
            if let Some(obg1) = obg1.last() {
                render_thrust(ui, obg1.thrust);
            }
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("pressure")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Pressure").color(text_color(false)).heading());
                });
            if let Some(obg1) = obg1.last() {
                render_pressure(ui, obg1.pressure);
            }
        });
    });
}
