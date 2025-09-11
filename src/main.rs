mod gui;
mod audio;
mod dsp;

use anyhow::Result;
use iced::{window, Settings, Size};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

fn main() -> Result<()> {
    // create shared audio settings and shutdown signal
    let audio_settings = Arc::new(Mutex::new(gui::AudioSettings::default()));
    let shutdown_signal = Arc::new(AtomicBool::new(false));
    
    // start audio processing in background thread
    let audio_settings_clone = audio_settings.clone();
    let audio_shutdown = shutdown_signal.clone();
    let audio_handle = thread::spawn(move || {
        if let Err(e) = audio::run_audio(audio_settings_clone, audio_shutdown) {
            eprintln!("Audio error: {}", e);
        }
    });

    // configure window settings
    let window_settings = window::Settings {
        size: Size::new(900.0, 700.0), // larger window for more controls
        position: window::Position::Centered,
        min_size: Some(Size::new(600.0, 500.0)),
        max_size: Some(Size::new(1400.0, 1000.0)),
        ..Default::default()
    };

    // configure application settings
    let app_settings = Settings {
        ..Default::default()
    };

    // run the GUI with shared audio settings
    let gui_result = gui::Montage::run(window_settings, app_settings, audio_settings);
    
    // signal audio thread to shutdown
    shutdown_signal.store(true, Ordering::Relaxed);
    
    // wait for audio thread to finish
    let _ = audio_handle.join();
    
    gui_result
}
