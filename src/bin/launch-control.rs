#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
// hide console window on Windows in release
#[cfg(feature = "novaview")]
use std::sync::Arc;

use std::time::Instant;

use clap::Parser;
use control_frontend::args::ProgramArgs;
use control_frontend::connection::Connection;
use control_frontend::consort::Consort;
use control_frontend::input::InputEvent;
use control_frontend::model::{Model, SharedIdGenerator};
use control_frontend::render::render;
use control_frontend::rqprotocol::Node;
#[cfg(feature = "novaview")]
use control_frontend::timestep::TimeStep;

use control_frontend::recorder::Recorder;

#[cfg(feature = "e32")]
use control_frontend::ebyte::E32Connection;
#[cfg(not(feature = "e32"))]
use control_frontend::ebytemock::E32Connection;

use egui::Key;

#[cfg(feature = "novaview")]
use egui_sdl2_platform::sdl2;
#[cfg(feature = "novaview")]
use egui_sdl2_platform::sdl2::joystick::Joystick;

use log::{error, info};

#[cfg(feature = "novaview")]
use sdl2::event::{Event, WindowEvent};

const SCREEN_WIDTH: u32 = 1024;
const SCREEN_HEIGHT: u32 = 600;

#[cfg(not(feature = "novaview"))]
const DEVICE: &str = "/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A50285BI-if00-port0";
#[cfg(feature = "novaview")]
const DEVICE: &str = "/dev/ttyAMA3";

fn serial_port_path() -> Option<String> {
    if std::path::Path::new(DEVICE).exists() {
        return Some(DEVICE.to_string());
    }
    serialport::available_ports().ok().and_then(|ports| {
        if ports.len() == 1 {
            Some(ports[0].port_name.clone())
        } else {
            None
        }
    })
}

#[cfg(feature = "eframe")]
fn main() -> Result<(), eframe::Error> {
    simple_logger::init_with_env().unwrap();

    let id_generator = SharedIdGenerator::default();
    let (me, target_red_queen) = (Node::LaunchControl, Node::RedQueen(b'A'));
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32)),
        ..Default::default()
    };
    let args = ProgramArgs::parse();
    let recorder = if args.dont_record {
        Recorder::new(None)
    } else {
        Recorder::new_with_default_file()
    };
    let recorder_path = recorder.path.clone();
    let conn = E32Connection::new(
        id_generator.clone(),
        me.clone(),
        target_red_queen.clone(),
        recorder,
    )
    .unwrap();

    eframe::run_native(
        "Launch Control",
        options,
        Box::new(|_cc| {
            Box::new(LaunchControlApp::new(
                id_generator,
                conn,
                args,
                recorder_path,
            ))
        }),
    )
}

struct LaunchControlApp<C, Id>
where
    C: Connection,
    Id: Iterator<Item = usize>,
{
    model: Model<C, Id>,
}

impl<C: Connection, Id: Iterator<Item = usize>> LaunchControlApp<C, Id> {
    fn new(id_generator: Id, conn: C, args: ProgramArgs, recorder_path: Option<PathBuf>) -> Self {
        let (me, target_red_queen) = (Node::LaunchControl, Node::RedQueen(b'A'));
        let start_time = Instant::now();

        let consort =
            Consort::new_with_id_generator(me, target_red_queen, start_time, id_generator);
        let port_path = args
            .port
            .or_else(|| serial_port_path())
            .expect("No serial port found");
        info!("Opening E32 {}", port_path);
        let model = Model::new(
            consort,
            conn,
            start_time,
            &port_path,
            &args.gain,
            args.start_with_launch_control,
            recorder_path,
        );

        Self { model }
    }

    #[cfg(feature = "novaview")]
    fn update(&mut self, input_events: &Vec<InputEvent>, ctx: &egui::Context) {
        self.model.drive(Instant::now()).unwrap();
        // Get the egui context and begin drawing the frame
        // Draw an egui window
        egui::Area::new("launch_control")
            .fixed_pos([0.0, 0.0])
            .constrain(true)
            .movable(false)
            .show(&ctx, |ui| {
                render(ui, &self.model);
            });
        self.model.process_input_events(&input_events);
    }
}

#[cfg(feature = "eframe")]
impl<C: Connection, Id: Iterator<Item = usize>> eframe::App for LaunchControlApp<C, Id> {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut input_events = vec![];
        ctx.input(|i| {
            if i.key_pressed(Key::ArrowRight) {
                input_events.push(InputEvent::Right(10));
            }
            if i.key_pressed(Key::ArrowLeft) {
                input_events.push(InputEvent::Left(10));
            }
            if i.key_pressed(Key::Enter) {
                input_events.push(InputEvent::Enter);
            }
            if i.key_pressed(Key::Space) {
                input_events.push(InputEvent::Enter);
            }
            if i.key_pressed(Key::Backspace) {
                input_events.push(InputEvent::Back);
            }
            if i.key_pressed(Key::Escape) {
                frame.close();
            }
        });

        self.model.drive(Instant::now()).unwrap();
        // Get the egui context and begin drawing the frame
        // Draw an egui window
        egui::Area::new("launch_control")
            .fixed_pos([0.0, 0.0])
            .constrain(true)
            .movable(false)
            .show(&ctx, |ui| {
                render(ui, &self.model);
            });
        self.model.process_input_events(&input_events);
    }
}

#[cfg(feature = "novaview")]
fn open_joystick(sdl: &sdl2::Sdl) -> Option<Joystick> {
    let subsystem = match sdl.joystick() {
        Ok(s) => s,
        Err(e) => {
            error!("Can't open joystick subsystem, {}", e);
            return None;
        }
    };
    let num_sticks = match subsystem.num_joysticks() {
        Ok(n) => n,
        Err(e) => {
            error!("Can't enumerate joysticks, {}", e);
            return None;
        }
    };
    let mut tiny_usb_device = None;
    for i in 0..num_sticks {
        match subsystem.name_for_index(i) {
            Ok(name) => {
                info!("Found stick {}", name);
                if name == "TinyUSB Device" {
                    tiny_usb_device = Some(i);
                }
            }
            Err(e) => {
                error!("Can't enumerate joysticks, {}", e);
            }
        }
    }
    if let Some(num) = tiny_usb_device {
        let joystick = match subsystem.open(num) {
            Ok(s) => s,
            Err(e) => {
                error!("Can't open joystick, {}", e);
                return None;
            }
        };
        return Some(joystick);
    }
    None
}

#[cfg(feature = "novaview")]
fn get_input_events(
    event_pump: &mut sdl2::EventPump,
    platform: &mut egui_sdl2_platform::Platform,
    sdl: &sdl2::Sdl,
    video: &mut sdl2::VideoSubsystem,
    window: &sdl2::video::Window,
    joystick: &mut Option<JoystickProcessor>,
) -> (bool, Vec<InputEvent>) {
    let mut input_events = vec![];
    let mut quit = false;
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
                        quit = true;
                    }
                }
            }
            Event::KeyDown {
                keycode: Some(sdl2::keyboard::Keycode::Escape),
                ..
            } => quit = true,
            Event::KeyDown { keycode, .. } => {
                if let Some(keycode) = keycode {
                    match keycode {
                        sdl2::keyboard::Keycode::Space => {
                            input_events.push(InputEvent::Enter);
                        }
                        sdl2::keyboard::Keycode::Return => {
                            input_events.push(InputEvent::Enter);
                        }
                        sdl2::keyboard::Keycode::Backspace => {
                            input_events.push(InputEvent::Back);
                        }
                        sdl2::keyboard::Keycode::Left => {
                            input_events.push(InputEvent::Left(10));
                        }
                        sdl2::keyboard::Keycode::Right => {
                            input_events.push(InputEvent::Right(10));
                        }
                        sdl2::keyboard::Keycode::S => input_events.push(InputEvent::Send),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        // Let the egui platform handle the event
        platform.handle_event(&event, sdl, video);
    }
    if let Some(joystick) = joystick {
        joystick.produce_events(&mut input_events);
    }

    (quit, input_events)
}

#[cfg(feature = "novaview")]
struct JoystickProcessor {
    joystick: Joystick,
    position: i64,
    trigger: i64,
    right_pressed: bool,
    left_pressed: bool,
}

#[cfg(feature = "novaview")]
impl JoystickProcessor {
    pub fn new(joystick: Joystick) -> Self {
        Self {
            joystick,
            position: 0,
            trigger: 0,
            right_pressed: false,
            left_pressed: false,
        }
    }

    pub fn produce_events(&mut self, input_events: &mut Vec<InputEvent>) {
        let axis0_value = self.joystick.axis(0).unwrap();
        // deadzone
        if axis0_value.abs() > 10 {
            self.position += axis0_value as i64;
        }
        if (self.trigger - self.position).abs() > 1000_000 / 40 {
            let diff = self.trigger - self.position;
            if diff > 0 {
                input_events.push(InputEvent::Right(10));
            } else {
                input_events.push(InputEvent::Left(10));
            }
            self.trigger = self.position;
        }
        let lbp = self.joystick.button(1).unwrap();
        let rbp = self.joystick.button(0).unwrap();
        if !self.left_pressed && lbp {
            input_events.push(InputEvent::Back);
        }
        self.left_pressed = lbp;
        if !self.right_pressed && rbp {
            input_events.push(InputEvent::Enter);
        }
        self.right_pressed = rbp;
    }
}

#[cfg(feature = "novaview")]
async fn run() -> anyhow::Result<()> {
    simple_logger::init_with_env().unwrap();
    let id_generator = SharedIdGenerator::default();
    let (me, target_red_queen) = (Node::LaunchControl, Node::RedQueen(b'A'));
    let args = ProgramArgs::parse();
    let recorder = Recorder::new(None);
    let conn = E32Connection::new(
        id_generator.clone(),
        me.clone(),
        target_red_queen.clone(),
        recorder,
    )
    .unwrap();
    let mut app = LaunchControlApp::new(id_generator, conn, args, None);

    // Initialize sdl
    let sdl = sdl2::init().map_err(|e| anyhow::anyhow!("Failed to create sdl context: {}", e))?;
    let mouse = sdl.mouse();
    let mut joystick = open_joystick(&sdl).and_then(|j| Some(JoystickProcessor::new(j)));

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
    let color = [0.0, 0.0, 0.0, 1.0];
    // Get the time before the loop started
    let start_time = Instant::now();
    let mut timestep = TimeStep::new();

    'main: loop {
        // Update the time
        let (quit, input_events) = get_input_events(
            &mut event_pump,
            &mut platform,
            &sdl,
            &mut video,
            &window,
            &mut joystick,
        );
        if quit {
            break 'main;
        }

        platform.update_time(start_time.elapsed().as_secs_f64());
        let ctx = platform.context();
        mouse.show_cursor(false);
        app.update(&input_events, &ctx);

        // Stop drawing the egui frame and get the full output
        let full_output = platform.end_frame(&mut video)?;
        // Get the paint jobs
        let paint_jobs = platform.tessellate(&full_output);
        let pj = paint_jobs.as_slice();

        // unsafe {
        //     painter.gl().clear_color(color[0], color[1], color[2], 1.0);
        //     painter.gl().clear(gl::COLOR_BUFFER_BIT);
        // }

        let size = window.size();
        painter.paint_and_update_textures([size.0, size.1], 1.0, pj, &full_output.textures_delta);
        window.gl_swap_window();
        timestep.run_this(|_| {});
    }
    Ok(())
}

#[cfg(feature = "novaview")]
fn main() -> anyhow::Result<()> {
    pollster::block_on(run())?;
    Ok(())
}
