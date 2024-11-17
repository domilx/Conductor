use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use std::time::Duration;

use wry::application::dpi::LogicalSize;
use wry::application::event::{Event, WindowEvent};
use wry::application::event_loop::{ControlFlow, EventLoop};
use wry::application::window::WindowBuilder;
use wry::webview::WebViewBuilder;

// Your module declarations
mod cfg;
mod input;
mod ipc;
mod keys;
mod panic;
mod resources;
mod scrn;
mod state;
mod util;
mod webserver;

// Use statements for your modules
use crate::state::State;
use cfg::Config;
use ds::DsMode;
use ipc::*;
use webserver::SetAddr;

const PERCENT_WIDTH: f64 = 0.7906295754026355;
const PERCENT_HEIGHT: f64 = 0.42;

fn main() -> wry::Result<()> {
    env_logger::init();
    let mut cfg = confy::load::<Config>("conductor").unwrap();

    if std::env::var("RUST_BACKTRACE").is_err() {
        std::panic::set_hook(Box::new(panic::hook));
    }

    // Display an error message and exit if on Windows
    #[cfg(target_os = "windows")]
    {
        use tinyfiledialogs::{message_box_ok, MessageBoxIcon};
        message_box_ok(
            "Unsupported Environment",
            "The Conductor Driver Station is not supported on your operating system. Please use the NI Driver Station instead.\n\nThis application will now terminate.",
            MessageBoxIcon::Error,
        );
        std::process::exit(1);
    }

    let state = Arc::new(RwLock::new(State::new()));
    let end_state = state.clone();
    let (tx, rx) = mpsc::channel();
    let (stdout_tx, stdout_rx) = mpsc::channel();

    let port = webserver::launch_webserver(state.clone(), tx, stdout_tx);
    println!("Webserver launched on port {}", port);

    let (width, height) = scrn::screen_resolution();
    println!("Detected Resolution {} {}", width, height);

    // Create the event loop
    let event_loop = EventLoop::new();

    // Create the main window
    let main_window = WindowBuilder::new()
        .with_title("Conductor DS")
        .with_inner_size(LogicalSize::new(
            width as f64 * PERCENT_WIDTH,
            height as f64 * PERCENT_HEIGHT,
        ))
        .build(&event_loop)?;

    // Create the main webview
    let webview = WebViewBuilder::new(main_window)?
        .with_url(&format!("http://localhost:{}", port))?
        .with_initialization_script(&format!("window.startapp({});", port))
        .with_ipc_handler(move |_window, _ipc_payload| {
            // Handle IPC messages from the frontend if needed
        })
        .build()?;

    // For Linux, create a separate window for the robot console
    #[cfg(target_os = "linux")]
    let stdout_window = WindowBuilder::new()
        .with_title("Robot Console")
        .with_inner_size(LogicalSize::new(650.0, 650.0))
        .build(&event_loop)?;

    #[cfg(target_os = "linux")]
    let stdout_wv = WebViewBuilder::new(stdout_window)?
        .with_url(&format!("http://localhost:{}/stdout", port))?
        .with_initialization_script(&format!("window.startapp({});", port))
        .build()?;

    let addr = rx.recv().unwrap();

    #[cfg(target_os = "linux")]
    {
        let stdout_addr = stdout_rx.recv().unwrap();
        addr.do_send(SetAddr { addr: stdout_addr });
    }

    state.write().unwrap().wire_stdout(addr.clone());

    if cfg.team_number != 0 {
        addr.do_send(Message::UpdateTeamNumber {
            team_number: cfg.team_number,
            from_backend: true,
        });
        state.write().unwrap().update_ds(cfg.team_number);
    }

    // Bind key events
    let keybindings_enabled = keys::bind_keys(state.clone(), addr.clone());
    addr.do_send(Message::Capabilities {
        backend_keybinds: keybindings_enabled,
    });

    // Start the input thread
    input::input_thread(addr.clone());

    // Spawn a thread to send periodic robot state updates
    {
        let state = state.clone();
        let addr = addr.clone();
        thread::spawn(move || loop {
            let msg = {
                let state = state.read().unwrap();
                let ds = &state.ds;
                let comms = ds.trace().is_connected();
                let code = ds.trace().is_code_started();
                let sim = ds.ds_mode() == DsMode::Simulation;
                let joysticks = input::JS_STATE
                    .get()
                    .unwrap()
                    .read()
                    .unwrap()
                    .has_joysticks();
                let voltage = ds.battery_voltage();

                Message::RobotStateUpdate {
                    comms_alive: comms,
                    code_alive: code,
                    simulator: sim,
                    joysticks,
                    voltage,
                }
            };

            addr.do_send(msg);
            thread::sleep(Duration::from_millis(50));
        });
    }

    // Run the event loop, moving the webview into the closure to keep it alive
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Keep the webview alive
        let _ = &webview;

        // Handle events for the webview
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                // Exit the application when the main window is closed
                if window_id == webview.window().id() {
                    *control_flow = ControlFlow::Exit;
                }

                #[cfg(target_os = "linux")]
                if window_id == stdout_wv.window().id() {
                    // Handle closing of the stdout window if needed
                }
            }
            Event::MainEventsCleared => {
                // Perform periodic tasks here if necessary
            }
            _ => {}
        }
    });

    // Update and store the team number before exiting
    cfg.team_number = end_state.read().unwrap().ds.team_number();
    log::info!("Updating team number to {}", cfg.team_number);
    confy::store("conductor", cfg).unwrap();

    Ok(())
}
