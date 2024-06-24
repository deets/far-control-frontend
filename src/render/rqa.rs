use epaint::Color32;
use std::time::Duration;
use uom::si::{
    f64::{Force, Pressure},
    force::kilonewton,
    pressure::bar,
};

use egui::{
    plot::{Legend, Line, Plot, PlotPoints},
    RichText, Ui,
};

use crate::observables::rqa::{ObservablesGroup1, ObservablesGroup2, RecordingState};

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

fn render_recording_state(ui: &mut Ui, recording_state: &RecordingState) {
    let (text, color) = match &recording_state {
        RecordingState::Unknown => ("Unknown".to_string(), Color32::DARK_GRAY),
        RecordingState::Error(text) => (text.clone(), Color32::RED),
        RecordingState::Pause => ("Pause".to_string(), Color32::DARK_GRAY),
        RecordingState::Recording(filename) => (filename.clone(), Color32::WHITE),
    };
    ui.label(RichText::new(text).heading().color(color));
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
            egui::SidePanel::left("records")
                .resizable(false)
                .show_separator_line(false)
                .frame(clear_frame())
                .resizable(false)
                .exact_width(ui.available_width() / 5.0)
                .show_inside(ui, |ui| {
                    ui.label(
                        RichText::new("Records written")
                            .color(text_color(false))
                            .heading(),
                    );
                });
            ui.label(
                RichText::new(
                    obg2.clone()
                        .map_or("--".to_string(), |obg2| format!("{}", obg2.records)),
                )
                .heading()
                .color(Color32::WHITE),
            );
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
            ui.label(
                RichText::new(
                    obg2.clone()
                        .map_or("--".to_string(), |obg2| format!("{}", obg2.anomalies)),
                )
                .heading()
                .color(Color32::WHITE),
            );
        });
        egui::SidePanel::left("thrust_plot")
            .resizable(false)
            .show_separator_line(false)
            .frame(clear_frame())
            .resizable(false)
            .exact_width(ui.available_width() / 2.0)
            .show_inside(ui, |ui| {
                let plot = Plot::new("thrust_plot").legend(Legend::default());
                let mut plot_points = PlotPoints::default();
                if obg1.len() >= 2 {
                    let start = obg1.first().unwrap().uptime;
                    let points: Vec<[f64; 2]> = obg1
                        .iter()
                        .map(|item| {
                            [
                                (item.uptime - start).as_secs_f64(),
                                item.thrust.get::<kilonewton>(),
                            ]
                        })
                        .collect();
                    plot_points = points.into();
                }
                plot.show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(plot_points)
                            .color(Color32::from_rgb(100, 150, 250))
                            .style(egui::plot::LineStyle::Solid)
                            .name("Thrust"),
                    );
                })
                .response
            });
        egui::SidePanel::left("pressure_plot")
            .resizable(false)
            .show_separator_line(false)
            .frame(clear_frame())
            .resizable(false)
            .exact_width(ui.available_width())
            .show_inside(ui, |ui| {
                let plot = Plot::new("pressure_plot").legend(Legend::default());
                let mut plot_points = PlotPoints::default();
                if obg1.len() >= 2 {
                    let start = obg1.first().unwrap().uptime;
                    let points: Vec<[f64; 2]> = obg1
                        .iter()
                        .map(|item| {
                            [
                                (item.uptime - start).as_secs_f64(),
                                item.pressure.get::<uom::si::pressure::hectopascal>(),
                            ]
                        })
                        .collect();
                    plot_points = points.into();
                }
                plot.show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(plot_points)
                            .color(Color32::from_rgb(100, 150, 250))
                            .style(egui::plot::LineStyle::Solid)
                            .name("Pressure"),
                    );
                })
                .response
            });
    });
}
