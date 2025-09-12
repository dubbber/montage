use std::sync::{Arc, Mutex};

pub struct DspProcessor {
    /// shared pitch control (playback speed)
    pitch: Arc<Mutex<f32>>,
    /// dual ring buffers for crossfading
    ring_buffer_a: Vec<f32>,
    ring_buffer_b: Vec<f32>,
    write_index: usize,
    read_index_a: f32,
    read_index_b: f32,
    /// crossfade position for smooth transitions
    crossfade_pos: f32,
    crossfade_step: f32,
    /// multi-stage low-pass filters for better anti-aliasing
    filter_state_1: f32,
    filter_state_2: f32,
    /// DC blocking filter
    dc_filter_x: f32,
    dc_filter_y: f32,
    /// smoothing for pitch changes
    current_pitch: f32,
    target_pitch: f32,
}

impl DspProcessor {
    pub fn new(pitch: Arc<Mutex<f32>>) -> Self {
        Self {
            pitch,
            ring_buffer_a: vec![0.0; 128], // smaller buffers for lower latency
            ring_buffer_b: vec![0.0; 128],
            write_index: 0,
            read_index_a: 0.0,
            read_index_b: 1024.0, // smaller offset for lower latency
            crossfade_pos: 0.0,
            crossfade_step: 0.005, // faster crossfade for lower latency
            filter_state_1: 0.0,
            filter_state_2: 0.0,
            dc_filter_x: 0.0,
            dc_filter_y: 0.0,
            current_pitch: 1.0,
            target_pitch: 1.0,
        }
    }

    /// process audio buffer (mono, f32) with optimized quality
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        // safely get pitch value with error handling
        self.target_pitch = match self.pitch.lock() {
            Ok(pitch) => (*pitch).clamp(0.5, 2.0), // more conservative pitch range
            Err(_) => {
                eprintln!("Failed to lock pitch mutex, using default value");
                1.0
            }
        };

        // ensure we don't process more samples than available
        let process_len = input.len().min(output.len());
        
        for i in 0..process_len {
            let in_sample = input[i];
            
            // faster pitch smoothing for lower latency
            self.current_pitch += (self.target_pitch - self.current_pitch) * 0.01;
            
            // apply multi-stage low-pass filtering for better anti-aliasing
            let filtered_input = self.multi_stage_filter(in_sample);
            
            // store filtered input in both ring buffers
            self.ring_buffer_a[self.write_index] = filtered_input;
            self.ring_buffer_b[self.write_index] = filtered_input;
            self.write_index = (self.write_index + 1) % self.ring_buffer_a.len();
            
            // calculate read step based on current pitch
            let read_step = 1.0 / self.current_pitch;
            
            // update read indices
            self.read_index_a += read_step;
            self.read_index_b += read_step;
            
            // wrap read indices
            if self.read_index_a >= self.ring_buffer_a.len() as f32 {
                self.read_index_a -= self.ring_buffer_a.len() as f32;
            }
            if self.read_index_b >= self.ring_buffer_b.len() as f32 {
                self.read_index_b -= self.ring_buffer_b.len() as f32;
            }
            
            // read samples with cubic interpolation for smoother sound
            let sample_a = self.cubic_interpolated_read(&self.ring_buffer_a, self.read_index_a);
            let sample_b = self.cubic_interpolated_read(&self.ring_buffer_b, self.read_index_b);
            
            // crossfade between the two buffers for smoother transitions
            let crossfade_weight = (self.crossfade_pos.sin() + 1.0) * 0.5;
            let crossfaded_sample = sample_a * (1.0 - crossfade_weight) + sample_b * crossfade_weight;
            
            // update crossfade position
            self.crossfade_pos += self.crossfade_step;
            if self.crossfade_pos >= std::f32::consts::PI * 2.0 {
                self.crossfade_pos -= std::f32::consts::PI * 2.0;
            }
            
            // apply DC blocking filter to remove DC offset
            let dc_blocked = self.dc_blocking_filter(crossfaded_sample);
            
            // apply gentle compression with softer knee
            let compressed_sample = self.advanced_soft_compress(dc_blocked);
            
            // mix with dry signal for more natural sound
            let dry_wet_mix = 0.8; // 80% processed, 20% dry
            let mixed_sample = compressed_sample * dry_wet_mix + filtered_input * (1.0 - dry_wet_mix);
            
            output[i] = mixed_sample;
        }
        
        // fill remaining output with silence if output is longer than input
        for sample in output[process_len..].iter_mut() {
            *sample = 0.0;
        }
    }
    
    /// multi-stage low-pass filter for better anti-aliasing
    fn multi_stage_filter(&mut self, input: f32) -> f32 {
        // first stage - aggressive filtering
        self.filter_state_1 = self.filter_state_1 * 0.85 + input * 0.15;
        // second stage - gentler filtering
        self.filter_state_2 = self.filter_state_2 * 0.9 + self.filter_state_1 * 0.1;
        self.filter_state_2
    }
    
    /// cubic interpolation for smoother sample reading
    fn cubic_interpolated_read(&self, buffer: &[f32], read_index: f32) -> f32 {
        let index = read_index.floor() as usize;
        let fraction = read_index.fract();
        
        // get 4 points for cubic interpolation
        let i0 = (index + buffer.len() - 1) % buffer.len();
        let i1 = index % buffer.len();
        let i2 = (index + 1) % buffer.len();
        let i3 = (index + 2) % buffer.len();
        
        let y0 = buffer[i0];
        let y1 = buffer[i1];
        let y2 = buffer[i2];
        let y3 = buffer[i3];
        
        // cubic interpolation (Catmull-Rom spline)
        let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c = -0.5 * y0 + 0.5 * y2;
        let d = y1;
        
        ((a * fraction + b) * fraction + c) * fraction + d
    }
    
    /// DC blocking filter to remove DC offset
    fn dc_blocking_filter(&mut self, input: f32) -> f32 {
        let output = input - self.dc_filter_x + 0.995 * self.dc_filter_y;
        self.dc_filter_x = input;
        self.dc_filter_y = output;
        output
    }
    
    /// advanced soft compression with smoother knee
    fn advanced_soft_compress(&self, input: f32) -> f32 {
        let threshold = 0.7;
        let ratio = 0.3;
        let knee_width = 0.1;
        
        let abs_input = input.abs();
        let sign = if input >= 0.0 { 1.0 } else { -1.0 };
        
        if abs_input <= threshold - knee_width {
            // below threshold - no compression
            input
        } else if abs_input >= threshold + knee_width {
            // above threshold - full compression
            let excess = abs_input - threshold;
            let compressed_excess = excess * ratio;
            sign * (threshold + compressed_excess)
        } else {
            // in knee region - smooth transition
            let knee_ratio = (abs_input - (threshold - knee_width)) / (2.0 * knee_width);
            let smooth_ratio = knee_ratio * knee_ratio * (3.0 - 2.0 * knee_ratio); // smoothstep
            let current_ratio = 1.0 - smooth_ratio * (1.0 - ratio);
            let excess = abs_input - threshold;
            sign * (threshold + excess * current_ratio)
        }
    }
}
