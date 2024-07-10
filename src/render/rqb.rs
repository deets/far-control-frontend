use emath::Vec2;
use epaint::{Color32, Shadow};

use egui::{Frame, Id, RichText, Sense, Ui};

use crate::{
    connection::Connection,
    model::Model,
    observables::rqb::PyroStatus,
    rqprotocol::Node,
    telemetry::parser::rq2::{IMUPacket, IgnitionSMState, TelemetryData},
};

use super::{clear_frame, text_color};

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

fn dark_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(text_color(false)).heading());
}

fn flatten_data(data: Option<&Vec<TelemetryData>>) -> (Option<IMUPacket>, Option<IgnitionSMState>) {
    let mut imu = None;
    let mut ism = None;
    if let Some(data) = data {
        for packet in data {
            match packet {
                TelemetryData::Ignition(d) => {
                    ism = Some(d.clone());
                }
                TelemetryData::IMU(d) => {
                    imu = Some(d.clone());
                }
            }
        }
    }
    (imu, ism)
}

fn render_vector(ui: &mut Ui, id: Id, prefix: &str, v: (f32, f32, f32)) {
    //    let min_col_width = ui.available_width() / 3.0;
    egui::Grid::new(id)
        .striped(false)
        //        .min_col_width(min_col_width)
        .show(ui, |ui| {
            dark_label(ui, &format!("{}x:{:3.3}", prefix, v.0));
            dark_label(ui, &format!("{}y:{:3.3}", prefix, v.1));
            dark_label(ui, &format!("{}z:{:3.3}", prefix, v.2));
        });
}

fn render_redqueen(ui: &mut Ui, name: &str, node: Node, data: Option<&Vec<TelemetryData>>) {
    let (imu_data, ignition_sm_state) = flatten_data(data);
    let base_id: Id = name.to_string().into();

    egui::Grid::new(base_id.with("outer grid"))
        .striped(false)
        .show(ui, |ui| {
            dark_label(ui, name);
            ui.end_row();
            dark_label(ui, "State");
            if let Some(state) = ignition_sm_state {
                dark_label(ui, &format!("{:?}", state));
            }
            ui.end_row();
            dark_label(ui, "Acc");
            if let Some(state) = &imu_data {
                render_vector(
                    ui,
                    base_id.with("acc"),
                    "a",
                    (state.imu.acc_x, state.imu.acc_y, state.imu.acc_z),
                );
            }
            ui.end_row();
            dark_label(ui, "Gyr");
            if let Some(state) = &imu_data {
                render_vector(
                    ui,
                    base_id.with("gyr"),
                    "g",
                    (state.imu.gyr_x, state.imu.gyr_y, state.imu.gyr_z),
                );
            }
        });
}

fn lined_frame() -> Frame {
    egui::containers::Frame {
        rounding: egui::Rounding::default(),
        fill: Color32::TRANSPARENT,
        stroke: egui::Stroke::new(2.0, Color32::WHITE),
        inner_margin: 4.0.into(),
        outer_margin: 2.0.into(),
        shadow: Shadow::NONE,
    }
}

pub fn render_observables<C, Id>(ui: &mut Ui, model: &Model<C, Id>)
where
    C: Connection,
    Id: Iterator<Item = usize>,
{
    egui::SidePanel::left("RQs")
        .resizable(false)
        .show_separator_line(false)
        .frame(clear_frame())
        .resizable(false)
        .exact_width(ui.available_width() / 2.0)
        .show_inside(ui, |ui| {
            let nodes = model.registered_nodes();
            let mut rqs: Vec<_> = nodes
                .iter()
                .filter(|n| match n {
                    Node::RedQueen(_) => true,
                    _ => false,
                })
                .collect();
            rqs.sort_by(|a, b| {
                let Node::RedQueen(a) = a else {
                    panic!("can't happen")
                };
                let Node::RedQueen(b) = b else {
                    panic!("can't happen")
                };
                a.cmp(b)
            });
            let mut count = rqs.len();
            for rq in rqs {
                let Node::RedQueen(c) = rq else {
                    panic!("can't happen")
                };
                let name = format!("RQ{}", unsafe { std::str::from_utf8_unchecked(&[*c]) });
                egui::TopBottomPanel::top(name.clone())
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(lined_frame())
                    .resizable(false)
                    .exact_height(ui.available_height() / count as f32)
                    .show_inside(ui, |ui| {
                        render_redqueen(ui, &name, rq.clone(), model.telemetry_data_for_node(rq));
                    });
                count -= 1;
            }
        });
    egui::SidePanel::right("FDs")
        .resizable(false)
        .show_separator_line(false)
        .frame(clear_frame())
        .resizable(false)
        .exact_width(ui.available_width())
        .show_inside(ui, |ui| {
            ui.label(RichText::new("FDB").color(text_color(false)).heading());
        });
}
