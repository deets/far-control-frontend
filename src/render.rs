use egui::{FontFamily, FontTweak, RichText, Ui};

use crate::state::State;

// fn render_header(ui: &Ui, state: &State) {
//     let [w, _h] = ui.window_content_region_max();
//     ui.set_next_item_width(w / 2.0);
//     ui.button("Observables");
//     ui.label_text(label, text)
//     ui.same_line();
//     ui.set_next_item_width(w / 2.0);
//     ui.button("Launch Control");
// }

pub fn setup_custom_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();

    // Install my own font (maybe supporting non-latin characters).
    // .ttf and .otf files supported.
    fonts.font_data.insert(
        "amiga4ever".to_owned(),
        egui::FontData::from_static(include_bytes!("../resources/amiga4ever-pro2.ttf")).tweak(
            FontTweak {
                scale: 1.2,            // make it smaller
                y_offset_factor: 0.07, // move it down slightly
                y_offset: 0.0,
            },
        ),
    );
    // Put my font first (highest priority) for proportional text:
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "amiga4ever".to_owned());

    // Put my font as last fallback for monospace:
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "amiga4ever".to_owned());

    // Tell egui to use these fonts:
    for family in &fonts.families {
        println!("family: {:?}", family);
    }
    ctx.set_fonts(fonts);
}

pub fn render(ui: &mut Ui, state: &State) {
    ui.label("Hello, world!");
    if ui.button("Greet").clicked() {
        println!("Hello, world!");
    }
    ui.horizontal(|ui| {
        ui.label("Color: ");
    });
}
