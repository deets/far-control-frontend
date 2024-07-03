use egui::{ProgressBar, Sense, Ui};
use emath::Align2;
use epaint::{Color32, FontId};
use palette::{Gradient, LinSrgb};

use crate::{
    layout::colors::{color32, kind_color32, Intensity, Kind},
    model::RFSilenceMode,
};

use super::{clear_frame, text_color};

fn render_progress(ui: &mut Ui, state: &RFSilenceMode) {
    let gradient = Gradient::new(vec![
        LinSrgb::new(0.0, 1.0, 0.0),
        LinSrgb::new(1.0, 1.0, 0.0),
        LinSrgb::new(1.0, 0.0, 0.0),
    ]);
    let progress = state.leave_radio_silence_progress();
    let color = color32(gradient.get(progress));

    let pbar = ProgressBar::new(progress).fill(match state {
        RFSilenceMode::LeaveRadioSilence { .. } => color,
        _ => Color32::DARK_GRAY,
    });
    ui.add(pbar);
}

fn render_header_text(ui: &mut Ui, state: RFSilenceMode) {
    let digit_font = FontId::new(48.0, egui::FontFamily::Monospace);
    let painter = ui.painter();
    let text = "Press Enter to enter RF Silence!";
    let galley = painter.layout_no_wrap(text.into(), digit_font.clone(), Color32::RED);
    let rect = galley.size();
    let (response, painter) = ui.allocate_painter(rect.into(), Sense::hover());

    painter.text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        text,
        digit_font,
        text_color(match state {
            RFSilenceMode::WaitForEnter => true,
            _ => false,
        }),
    );
    render_progress(ui, &state);
}

pub fn render_rf_silence(ui: &mut Ui, state: RFSilenceMode) {
    ui.horizontal(|ui| {
        egui::SidePanel::left("rf_silence")
            .resizable(false)
            .show_separator_line(false)
            .frame(clear_frame())
            .exact_width(ui.available_width())
            .show_inside(ui, |ui| {
                ui.vertical(|ui| {
                    render_header_text(ui, state);
                });
            })
    });
}
