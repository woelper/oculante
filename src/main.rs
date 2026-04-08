#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::sync::mpsc;

use clap::{Arg, Command};
use log::{debug, error};

use oculante::app::OculanteApp;
use oculante::appstate::OculanteState;
use oculante::scrubber::find_first_image_in_directory;
use oculante::utils::{Frame, Player};
use oculante::window_config::build_window_settings;

fn main() -> eframe::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    let _ = env_logger::try_init();

    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();
    let mut matches = Command::new("Oculante")
        .arg(
            Arg::new("INPUT")
                .help("Display this image")
                .multiple_values(true),
        )
        .arg(
            Arg::new("l")
                .short('l')
                .help("Listen on port")
                .takes_value(true),
        )
        .arg(
            Arg::new("stdin")
                .short('s')
                .id("stdin")
                .takes_value(false)
                .help("Load data from STDIN"),
        )
        .arg(
            Arg::new("chainload")
                .required(false)
                .takes_value(false)
                .short('c')
                .help("Chainload on Mac"),
        )
        .get_matches_from(args);

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        ..Default::default()
    };

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
        state.message_channel.0.clone(),
        state.persistent_settings.decoders,
    );

    // Parse input paths
    let paths_to_open: Vec<PathBuf> = matches
        .remove_many::<String>("INPUT")
        .unwrap_or_default()
        .map(PathBuf::from)
        .collect();

    if paths_to_open.len() == 1 {
        let location = paths_to_open.into_iter().next().unwrap();
        if location.is_dir() {
            if let Ok(first) = find_first_image_in_directory(&location) {
                state.is_loaded = false;
                state.player.load(&first);
                state.current_path = Some(first);
            }
        } else {
            state.is_loaded = false;
            state.player.load(&location);
            state.current_path = Some(location);
        }
    } else if paths_to_open.len() > 1 {
        let location = paths_to_open.first().unwrap();
        if location.is_dir() {
            if let Ok(first) = find_first_image_in_directory(location) {
                state.is_loaded = false;
                state.current_path = Some(first.clone());
                state.player.load_advanced(
                    &first,
                    Some(Frame::ImageCollectionMember(Default::default())),
                );
            }
        } else {
            state.is_loaded = false;
            state.current_path = Some(location.clone());
            state.player.load_advanced(
                location,
                Some(Frame::ImageCollectionMember(Default::default())),
            );
        }
        state.scrubber.fixed_paths = paths_to_open.iter().all(|p| p.is_file());
        state.scrubber.entries = paths_to_open;
        state.scrubber.wrap = state.persistent_settings.wrap_folder;
    }

    if matches.contains_id("stdin") {
        use std::io::Read;
        let mut input = vec![];
        if let Ok(bytes_read) = std::io::stdin().read_to_end(&mut input) {
            if bytes_read > 0 {
                match image::load_from_memory(&input) {
                    Ok(i) => {
                        let _ = state.texture_channel.0.send(Frame::new_reset(i));
                    }
                    Err(e) => error!("Error loading from stdin: {e}"),
                }
            }
        }
    }

    if let Some(port) = matches.value_of("l") {
        if let Ok(p) = port.parse::<i32>() {
            state.send_message_info(&format!("Listening on {p}"));
            oculante::net::recv(p, state.texture_channel.0.clone());
            state.current_path = Some(PathBuf::from(format!("network port {p}")));
            state.network_mode = true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = oculante::mac::launch();
    }

    let ws = build_window_settings();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([ws.width as f32, ws.height as f32])
            .with_min_inner_size([
                ws.min_size.map(|s| s.0).unwrap_or(100) as f32,
                ws.min_size.map(|s| s.1).unwrap_or(100) as f32,
            ])
            .with_title(ws.title)
            .with_decorations(ws.decorations),
        vsync: ws.vsync,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "oculante",
        options,
        Box::new(|_cc| Ok(Box::new(OculanteApp::new(state)))),
    )
}
