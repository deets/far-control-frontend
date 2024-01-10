#![feature(assert_matches, slice_pattern, exclusive_range_pattern)]
mod render;
mod rqp;
mod rqprotocol;
mod state;
mod timestep;
mod visualisation;
mod layout;
mod input;

use std::{sync::Arc, time::Instant};

use render::render;

use egui_glow::glow::HasContext;
use egui_sdl2_platform::sdl2;
use sdl2::event::{Event, WindowEvent};
use state::State;
use timestep::TimeStep;
use visualisation::setup_custom_fonts;


const SCREEN_WIDTH: u32 = 800;
const SCREEN_HEIGHT: u32 = 480;

async fn run() -> anyhow::Result<()> {
    // Initialize sdl
    let sdl = sdl2::init().map_err(|e| anyhow::anyhow!("Failed to create sdl context: {}", e))?;
    // Create the video subsystem
    let mut video = sdl
        .video()
        .map_err(|e| anyhow::anyhow!("Failed to initialize sdl video subsystem: {}", e))?;
    // Create the sdl window
    let window = video
        .window("Window", SCREEN_WIDTH, SCREEN_HEIGHT)
        .opengl()
        .position_centered()
        .build()?;
    // Get the sdl event pump
    let mut event_pump = sdl
        .event_pump()
        .map_err(|e| anyhow::anyhow!("Failed to get sdl event pump: {}", e))?;

    let _gl_context = window
        .gl_create_context()
        .expect("Failed to create GL context");

    let gl = unsafe {
        egui_glow::painter::Context::from_loader_function(|name| {
            video.gl_get_proc_address(name) as *const _
        })
    };
    let mut painter = egui_glow::Painter::new(Arc::new(gl), "", None).unwrap();

    // Create the egui + sdl2 platform
    let mut platform = egui_sdl2_platform::Platform::new(window.size())?;

    // The clear color
    let mut color = [0.0, 0.0, 0.0, 1.0];
    // Get the time before the loop started
    let start_time = Instant::now();
    let mut timestep = TimeStep::new();
    let mut state = State::default();
    let ctx = platform.context();
    setup_custom_fonts(&ctx);
    // The main loop
    'main: loop {
        // Update the time
        platform.update_time(start_time.elapsed().as_secs_f64());
        let ctx = platform.context();
        let mut input_events = vec![];

        // Get the egui context and begin drawing the frame
        // Draw an egui window
        egui::Area::new("launch_control")
            .fixed_pos([0.0, 0.0])
            .constrain(true)
            .movable(false)
            .show(&ctx, |ui| {
                render(ui, &state);
            });

        // Stop drawing the egui frame and get the full output
        let full_output = platform.end_frame(&mut video)?;
        // Get the paint jobs
        let paint_jobs = platform.tessellate(&full_output);
        let pj = paint_jobs.as_slice();

        unsafe {
            painter.gl().clear_color(color[0], color[1], color[2], 1.0);
            painter.gl().clear(gl::COLOR_BUFFER_BIT);
        }

        let size = window.size();
        painter.paint_and_update_textures([size.0, size.1], 1.0, pj, &full_output.textures_delta);
        window.gl_swap_window();
        timestep.run_this(|_| {});

        // Handle sdl events
        for event in event_pump.poll_iter() {
            // Handle sdl events
            match event {
                Event::Window {
                    window_id,
                    win_event,
                    ..
                } => {
                    if window_id == window.id() {
                        if let WindowEvent::Close = win_event {
                            break 'main;
                        }
                    }
                }
                Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => break 'main,
                Event::KeyDown {
                    keycode,
                    ..
                } => {
                    if let Some(keycode) = keycode {
                    println!("match keycode");
                        match keycode {
                            sdl2::keyboard::Keycode::Space => { input_events.push(input::Event::Enter); }
                            sdl2::keyboard::Keycode::Return => { input_events.push(input::Event::Enter); }
                            sdl2::keyboard::Keycode::Backspace => { input_events.push(input::Event::Back); }
                            sdl2::keyboard::Keycode::Left => { input_events.push(input::Event::Left(10)); }
                            sdl2::keyboard::Keycode::Right => { input_events.push(input::Event::Right(10)); }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            // Let the egui platform handle the event
            platform.handle_event(&event, &sdl, &video);
        }
        state.process_events(&input_events);
        if let Some(_fps) = timestep.frame_rate() {
            println!("{:?}", _fps);
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    pollster::block_on(run())?;
    Ok(())
}
