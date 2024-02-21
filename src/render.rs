use egui::epaint::Shadow;
use egui::{
    vec2, Align2, Color32, FontId, Frame, ProgressBar, RichText, Rounding, Sense, Stroke, Ui,
};
use emath::{pos2, Pos2};

use crate::connection::Connection;
use crate::model::{ControlArea, LaunchControlState, Mode, Model, StateProcessing};

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

fn render_launch_control(ui: &mut Ui, state: &LaunchControlState) {
    let (hi_a, lo_a, hi_b, lo_b) = state.digits();
    let (hi_a_hl, lo_a_hl, hi_b_hl, lo_b_hl) = state.highlights();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            egui::SidePanel::left("secret a left")
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Enter Secret A").heading());
                });
            render_digit(ui, hi_a, hi_a_hl);
            render_digit(ui, lo_a, lo_a_hl);
        });
        ui.horizontal(|ui| {
            egui::SidePanel::left("secret b left")
                .exact_width(ui.available_width() / 3.0)
                .show_inside(ui, |ui| {
                    ui.label(RichText::new("Enter Secret B").heading());
                });
            render_digit(ui, hi_b, hi_b_hl);
            render_digit(ui, lo_b, lo_b_hl);
        });
        let pbar =
            ProgressBar::new(state.prepare_ignition_progress() as f32 / 100.0).fill(Color32::RED);
        ui.add(pbar);
    });
}

fn render_body<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, state: &Model<C, Id>) {
    match state.mode {
        Mode::Observables(_state) => {}
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

pub fn render<C: Connection, Id: Iterator<Item = usize>>(ui: &mut Ui, model: &Model<C, Id>) {
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
