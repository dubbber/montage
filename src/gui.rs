use iced::widget::{Column, Container, Image, PickList, Row, Slider, Stack, Text};
use iced::{Element, Length, Alignment, Settings, Task, Color, Background, Border, Shadow, Vector};
use anyhow::Result;
use iced_wgpu::Renderer;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    PitchChanged(f32),
    SampleRateChanged(SampleRate),
    BufferSizeChanged(f32),
    DelayChanged(f32),
    Tick(Instant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Rate22050,
    Rate44100,
    Rate48000,
    Rate96000,
}

impl SampleRate {
    const ALL: [SampleRate; 4] = [
        SampleRate::Rate22050,
        SampleRate::Rate44100,
        SampleRate::Rate48000,
        SampleRate::Rate96000,
    ];

    pub fn to_hz(self) -> u32 {
        match self {
            SampleRate::Rate22050 => 22050,
            SampleRate::Rate44100 => 44100,
            SampleRate::Rate48000 => 48000,
            SampleRate::Rate96000 => 96000,
        }
    }
}

impl std::fmt::Display for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Hz", self.to_hz())
    }
}

#[derive(Debug, Clone)]
pub struct AudioSettings {
    pub pitch: f32,
    pub sample_rate: SampleRate,
    pub buffer_size: u32,
    pub delay_ms: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            pitch: 1.0,
            sample_rate: SampleRate::Rate44100,
            buffer_size: 256,
            delay_ms: 0.0,
        }
    }
}

pub struct VoiceChangerApp {
    settings: AudioSettings,
    shared_settings: Arc<Mutex<AudioSettings>>,
    buffer_size_slider: f32, // For slider (log scale)
    animation_time: f32,
    last_interaction: Instant,
    slider_animations: SliderAnimations,
}

#[derive(Debug, Clone)]
struct SliderAnimations {
    pitch_scale: f32,
    buffer_scale: f32,
    delay_scale: f32,
    pitch_glow: f32,
    buffer_glow: f32,
    delay_glow: f32,
}

impl Default for SliderAnimations {
    fn default() -> Self {
        Self {
            pitch_scale: 1.0,
            buffer_scale: 1.0,
            delay_scale: 1.0,
            pitch_glow: 0.0,
            buffer_glow: 0.0,
            delay_glow: 0.0,
        }
    }
}

impl VoiceChangerApp {
    fn new(shared_settings: Arc<Mutex<AudioSettings>>) -> (Self, Task<Message>) {
        let initial_settings = match shared_settings.lock() {
            Ok(settings) => settings.clone(),
            Err(_) => {
                eprintln!("Failed to lock settings mutex, using default");
                AudioSettings::default()
            }
        };
        
        // Convert buffer size to slider scale (log scale for better UX)
        let buffer_size_slider = (initial_settings.buffer_size as f32).log2();
        
        (
            Self {
                settings: initial_settings,
                shared_settings,
                buffer_size_slider,
                animation_time: 0.0,
                last_interaction: Instant::now(),
                slider_animations: SliderAnimations::default(),
            },
            Task::batch([
                Task::perform(async { Instant::now() }, Message::Tick),
            ]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PitchChanged(val) => {
                self.settings.pitch = val;
                self.last_interaction = Instant::now();
                self.slider_animations.pitch_scale = 1.2; // Squishy effect
                self.slider_animations.pitch_glow = 1.0;
            }
            Message::SampleRateChanged(rate) => {
                self.settings.sample_rate = rate;
                self.last_interaction = Instant::now();
            }
            Message::BufferSizeChanged(val) => {
                self.buffer_size_slider = val;
                self.settings.buffer_size = (2.0_f32.powf(val).round() as u32).max(64).min(2048);
                self.last_interaction = Instant::now();
                self.slider_animations.buffer_scale = 1.2;
                self.slider_animations.buffer_glow = 1.0;
            }
            Message::DelayChanged(val) => {
                self.settings.delay_ms = val;
                self.last_interaction = Instant::now();
                self.slider_animations.delay_scale = 1.2;
                self.slider_animations.delay_glow = 1.0;
            }
            Message::Tick(now) => {
                // Update animation time
                let _dt = now.duration_since(self.last_interaction).as_secs_f32();
                self.animation_time += 0.016; // ~60fps
                
                // Animate slider scales back to normal (squishy effect)
                self.slider_animations.pitch_scale = 
                    1.0 + (self.slider_animations.pitch_scale - 1.0) * 0.85;
                self.slider_animations.buffer_scale = 
                    1.0 + (self.slider_animations.buffer_scale - 1.0) * 0.85;
                self.slider_animations.delay_scale = 
                    1.0 + (self.slider_animations.delay_scale - 1.0) * 0.85;
                
                // Fade out glow effects
                self.slider_animations.pitch_glow *= 0.95;
                self.slider_animations.buffer_glow *= 0.95;
                self.slider_animations.delay_glow *= 0.95;
                
                return Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(16)).await;
                        Instant::now()
                    },
                    Message::Tick,
                );
            }
        }
        
        // Update shared settings
        if let Ok(mut shared_settings) = self.shared_settings.lock() {
            *shared_settings = self.settings.clone();
        } else {
            eprintln!("Failed to update shared settings");
        }
        
        Task::none()
    }

    fn view(&self) -> Element<'_, Message, iced::Theme, Renderer> {
        // Create animated, styled sliders with squishy effects
        let pitch_glow_intensity = self.slider_animations.pitch_glow;
        let buffer_glow_intensity = self.slider_animations.buffer_glow;
        let delay_glow_intensity = self.slider_animations.delay_glow;

        // Pitch control with animation
        let pitch_slider = Container::new(
            Slider::new(
                0.5..=2.0,
                self.settings.pitch,
                Message::PitchChanged,
            )
            .step(0.10)
        )
        .style(move |_theme| {
            container_style_with_glow(pitch_glow_intensity)
        });

        let pitch_section = Container::new(
            Column::new()
                .spacing(15)
                .push(
                    Text::new("Pitch Control")
                        .size(18)
                        .color(Color::from_rgb(0.8, 0.9, 1.0))
                )
                .push(
                    Text::new(format!("Pitch: {:.1}x", self.settings.pitch))
                        .size(14)
                        .color(Color::from_rgb(0.6, 0.8, 1.0))
                )
                .push(pitch_slider)
        )
        .padding(20)
        .style(move |_theme| {
            section_style_with_scale(self.slider_animations.pitch_scale)
        });

        // Sample rate control
        let sample_rate_picker = PickList::new(
            &SampleRate::ALL[..],
            Some(self.settings.sample_rate),
            Message::SampleRateChanged,
        )
        .style(|_theme, _status| {
            iced::widget::pick_list::Style {
                text_color: Color::from_rgb(0.9, 0.9, 1.0),
                placeholder_color: Color::from_rgb(0.6, 0.6, 0.8),
                handle_color: Color::from_rgb(0.5, 0.7, 0.9),
                background: Background::Color(Color::from_rgba(0.2, 0.3, 0.4, 0.8)),
                border: Border {
                    color: Color::from_rgb(0.4, 0.6, 0.8),
                    width: 2.0,
                    radius: 8.0.into(),
                },
            }
        });

        let sample_rate_section = Container::new(
            Column::new()
                .spacing(15)
                .push(
                    Text::new("Sample Rate")
                        .size(18)
                        .color(Color::from_rgb(0.8, 0.9, 1.0))
                )
                .push(sample_rate_picker)
        )
        .padding(20)
        .style(|_theme| section_style());

        // Buffer size control with animation
        let buffer_size_slider = Container::new(
            Slider::new(
                6.0..=11.0,
                self.buffer_size_slider,
                Message::BufferSizeChanged,
            )
            .step(0.1)
        )
        .style(move |_theme| {
            container_style_with_glow(buffer_glow_intensity)
        });

        let buffer_size_section = Container::new(
            Column::new()
                .spacing(15)
                .push(
                    Text::new("Buffer Size")
                        .size(18)
                        .color(Color::from_rgb(0.8, 0.9, 1.0))
                )
                .push(
                    Text::new(format!("Buffer: {} samples (~{:.1}ms @ {}Hz)", 
                        self.settings.buffer_size,
                        (self.settings.buffer_size as f32 / self.settings.sample_rate.to_hz() as f32) * 1000.0,
                        self.settings.sample_rate.to_hz()
                    ))
                    .size(12)
                    .color(Color::from_rgb(0.6, 0.8, 1.0))
                )
                .push(buffer_size_slider)
        )
        .padding(20)
        .style(move |_theme| {
            section_style_with_scale(self.slider_animations.buffer_scale)
        });

        // Delay control with animation
        let delay_slider = Container::new(
            Slider::new(
                0.0..=100.0,
                self.settings.delay_ms,
                Message::DelayChanged,
            )
            .step(1.0)
        )
        .style(move |_theme| {
            container_style_with_glow(delay_glow_intensity)
        });

        let delay_section = Container::new(
            Column::new()
                .spacing(15)
                .push(
                    Text::new("Output Delay")
                        .size(18)
                        .color(Color::from_rgb(0.8, 0.9, 1.0))
                )
                .push(
                    Text::new(format!("Delay: {:.0}ms", self.settings.delay_ms))
                        .size(14)
                        .color(Color::from_rgb(0.6, 0.8, 1.0))
                )
                .push(delay_slider)
        )
        .padding(20)
        .style(move |_theme| {
            section_style_with_scale(self.slider_animations.delay_scale)
        });

        // Layout controls in a grid
        let left_column = Column::new()
            .spacing(25)
            .width(Length::Fill)
            .push(pitch_section)
            .push(sample_rate_section);

        let right_column = Column::new()
            .spacing(25)
            .width(Length::Fill)
            .push(buffer_size_section)
            .push(delay_section);

        let controls_row = Row::new()
            .spacing(30)
            .push(left_column)
            .push(right_column);

        // Floating animation effect (for future use)
        let _float_offset = (self.animation_time * 2.0).sin() * 3.0;

        let background: Image = Image::new("assets/anime.jpeg")
            .width(Length::Fill)
            .height(Length::Fill);

        let content = Column::new()
            .spacing(30)
            .align_x(Alignment::Center)
            .push(
                Text::new("Voice Effects Control Panel")
                    .size(28)
                    .color(Color::from_rgb(1.0, 1.0, 1.0))
            )
            .push(controls_row);


        let container_element: Element<Message, iced::Theme, Renderer> = Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(30)
            .style(|_theme| {
                iced::widget::container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.05, 0.1, 0.2, 0.95))),
                    border: Border {
                        color: Color::from_rgba(0.3, 0.5, 0.8, 0.5),
                        width: 2.0,
                        radius: 15.0.into(),
                    },
                    shadow: Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                        offset: Vector::new(0.0, 5.0),
                        blur_radius: 15.0,
                    },
                    text_color: Some(Color::from_rgb(0.9, 0.9, 1.0)),
                }
            })
            .into();

        Stack::new()
            .push(background)
            .push(container_element)
            .into()
    }
}

// Custom styling functions
fn section_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgba(0.15, 0.2, 0.3, 0.8))),
        border: Border {
            color: Color::from_rgba(0.4, 0.6, 0.8, 0.6),
            width: 1.5,
            radius: 12.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        text_color: Some(Color::from_rgb(0.9, 0.9, 1.0)),
    }
}

fn section_style_with_scale(scale: f32) -> iced::widget::container::Style {
    let mut style = section_style();
    // Simulate scale effect with enhanced glow
    let glow_intensity = (scale - 1.0) * 5.0;
    style.border.color = Color::from_rgba(
        0.4 + glow_intensity * 0.3,
        0.6 + glow_intensity * 0.2,
        0.8 + glow_intensity * 0.1,
        0.6 + glow_intensity * 0.4,
    );
    style.shadow.blur_radius = 8.0 + glow_intensity * 10.0;
    style
}

fn container_style_with_glow(glow: f32) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgba(0.2, 0.3, 0.5, 0.3 + glow * 0.3))),
        border: Border {
            color: Color::from_rgba(0.5 + glow * 0.3, 0.7 + glow * 0.2, 0.9 + glow * 0.1, 0.8),
            width: 2.0 + glow * 2.0,
            radius: 8.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.3, 0.5, 0.8, glow * 0.5),
            offset: iced::Vector::new(0.0, 0.0),
            blur_radius: 5.0 + glow * 15.0,
        },
        text_color: Some(Color::from_rgb(0.9, 0.9, 1.0)),
    }
}

impl VoiceChangerApp {
    pub fn run(
        window_settings: iced::window::Settings,
        settings: Settings,
        shared_settings: Arc<Mutex<AudioSettings>>,
    ) -> Result<()> {
        iced::application(
            "Voice Effects Control Panel",
            VoiceChangerApp::update,
            VoiceChangerApp::view,
        )
        .settings(settings)
        .window(window_settings)
        .run_with(move || VoiceChangerApp::new(shared_settings))
        .map_err(|e| anyhow::anyhow!("GUI error: {}", e))
    }
}
