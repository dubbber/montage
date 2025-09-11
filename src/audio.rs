use crate::{dsp::DspProcessor, gui::AudioSettings};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::VecDeque;

pub fn run_audio(settings: Arc<Mutex<AudioSettings>>, shutdown_signal: Arc<AtomicBool>) -> Result<()> {
    let host = cpal::default_host();

    let input_device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;

    let output_device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No output device available"))?;

    // get initial settings
    let initial_settings = {
        let settings_lock = settings.lock().unwrap();
        settings_lock.clone()
    };

    let channels = 2; // force stereo for better compatibility
    let sample_rate = initial_settings.sample_rate.to_hz();
    let buffer_size = cpal::BufferSize::Fixed(initial_settings.buffer_size);

    // create DSP processor with pitch reference
    let pitch_ref = Arc::new(Mutex::new(initial_settings.pitch));
    let mut dsp = DspProcessor::new(pitch_ref.clone());

    // create delay buffer for output delay
    let max_delay_samples = (sample_rate as f32 * 0.1) as usize; // max 100ms delay
    let mut delay_buffer: VecDeque<f32> = VecDeque::with_capacity(max_delay_samples);
    
    let input_stream_config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size,
    };

    let output_stream_config = input_stream_config.clone();

    let err_fn = |err| {
        eprintln!("Audio stream error: {}", err);
    };

    // channel for audio data
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(4);

    // input stream
    let input_stream = input_device.build_input_stream(
        &input_stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // convert to mono signal
            let mut buffer = Vec::with_capacity(data.len() / channels);
            for frame in data.chunks(channels) {
                if !frame.is_empty() {
                    let mono_sample = if channels == 2 && frame.len() >= 2 {
                        (frame[0] + frame[1]) * 0.5
                    } else {
                        frame[0]
                    };
                    buffer.push(mono_sample);
                }
            }
            
            let _ = tx.try_send(buffer);
        },
        err_fn.clone(),
        None,
    )?;

    // output stream with settings monitoring
    let settings_clone = settings.clone();
    let output_stream = output_device.build_output_stream(
        &output_stream_config,
        move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // update settings from GUI
            let current_settings = {
                if let Ok(settings_lock) = settings_clone.try_lock() {
                    let settings = settings_lock.clone();
                    // update pitch in DSP
                    if let Ok(mut pitch_lock) = pitch_ref.try_lock() {
                        *pitch_lock = settings.pitch;
                    }
                    settings
                } else {
                    return; // skip this buffer if we can't get settings
                }
            };

            // calculate delay samples
            let delay_samples = ((current_settings.delay_ms / 1000.0) * sample_rate as f32) as usize;
            let delay_samples = delay_samples.min(max_delay_samples);

            match rx.try_recv() {
                Ok(input_buffer) => {
                    if !input_buffer.is_empty() {
                        let process_len = output.len().min(input_buffer.len() * channels);
                        let mono_len = process_len / channels;
                        
                        if mono_len > 0 {
                            // process audio
                            let mut processed_buffer = vec![0.0f32; mono_len];
                            dsp.process(&input_buffer[..mono_len], &mut processed_buffer);
                            
                            // apply delay
                            for (i, &sample) in processed_buffer.iter().enumerate() {
                                // add to delay buffer
                                delay_buffer.push_back(sample);
                                
                                // get delayed sample
                                let delayed_sample = if delay_buffer.len() > delay_samples {
                                    delay_buffer.pop_front().unwrap_or(0.0)
                                } else {
                                    0.0 // silence during initial delay buildup
                                };
                                
                                // output to both channels
                                if i * channels < output.len() {
                                    output[i * channels] = delayed_sample;
                                    if channels == 2 && i * channels + 1 < output.len() {
                                        output[i * channels + 1] = delayed_sample;
                                    }
                                }
                            }
                            
                            // fill remaining with silence
                            for sample in output[mono_len * channels..].iter_mut() {
                                *sample = 0.0;
                            }
                        } else {
                            // fill with silence
                            for sample in output.iter_mut() {
                                *sample = 0.0;
                            }
                        }
                    } else {
                        // fill with silence
                        for sample in output.iter_mut() {
                            *sample = 0.0;
                        }
                    }
                },
                Err(_) => {
                    // no input data, output delayed samples or silence
                    for i in 0..(output.len() / channels) {
                        let delayed_sample = if delay_buffer.len() > delay_samples {
                            delay_buffer.pop_front().unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        
                        if i * channels < output.len() {
                            output[i * channels] = delayed_sample;
                            if channels == 2 && i * channels + 1 < output.len() {
                                output[i * channels + 1] = delayed_sample;
                            }
                        }
                    }
                }
            }
        },
        err_fn,
        None,
    )?;

    input_stream.play()?;
    output_stream.play()?;

    println!("Audio streams running with configurable settings...");
    
    // keep streams alive until shutdown signal is received
    while !shutdown_signal.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    
    println!("Shutting down audio streams...");
    
    drop(input_stream);
    drop(output_stream);
    
    Ok(())
}
