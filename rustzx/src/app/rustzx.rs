//! Main application class module
//! Handles all platform-related, hardware-related stuff
//! and command-line interface

use crate::{
    app::{events::*, sound::*, video::*, settings::Settings},
    host::GuiHost,
};
use rustzx_core::{
    emulator::*,
    zx::constants::*,
    zx::tape::TapeImpl,
};
use std::{
    thread,
    time::{Duration, Instant},
};

/// max 100 ms interval in `max frames` speed mode
const MAX_FRAME_TIME: Duration = Duration::from_millis(100);

struct InstantStopwatch {
    timestamp: Instant,
}

impl Default for InstantStopwatch {
    fn default() -> Self {
        InstantStopwatch { timestamp: Instant::now() }
    }
}

impl Stopwatch for InstantStopwatch {
    fn reset(&mut self) {
        self.timestamp = Instant::now();
    }

    fn measure(&self) -> Duration {
        self.timestamp.elapsed()
    }
}

/// converts nanoseconds  to miliseconds
fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000f64
}

/// converts miliseconds to nanoseconds
fn ms_to_ns(s: f64) -> u64 {
    (s * 1_000_000_f64) as u64
}

/// returns frame length from given `fps`
fn frame_length(fps: usize) -> Duration {
    Duration::from_millis((1000 as f64 / fps as f64) as u64)
}

/// Application instance type
pub struct RustzxApp {
    /// main emulator object
    emulator: Emulator<GuiHost>,
    /// Sound rendering in a separate thread
    snd: Option<Box<dyn SoundDevice>>,
    video: Box<dyn VideoDevice>,
    events: Box<dyn EventDevice>,
    tex_border: TextureInfo,
    tex_canvas: TextureInfo,
    scale: u32,
}

impl RustzxApp {
    /// Starts application itself
    pub fn from_config(settings: Settings) -> anyhow::Result<RustzxApp> {
        let snd: Option<Box<dyn SoundDevice>> = if settings.sound_enabled {
            Some(Box::new(SoundSdl::new(&settings)))
        } else {
            None
        };
        let mut video = Box::new(VideoSdl::new(&settings));
        let tex_border = video.gen_texture(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
        let tex_canvas = video.gen_texture(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
        let scale = settings.scale as u32;
        let events = Box::new(EventsSdl::new(&settings));

        let mut host = GuiHost::from_settings(settings.to_rustzx_settings());

        if let Some(rom) = settings.rom.as_ref() {
            host = host.with_rom(rom)?;
        }
        if let Some(snapshot) = settings.sna.as_ref() {
            host = host.with_snapshot(snapshot)?;
        }
        if let Some(tape) = settings.tap.as_ref() {
            host = host.with_tape(tape)?;
        }

        let emulator = Emulator::new(host).map_err(|e| {
            anyhow::anyhow!("Failed to construct emulator: {}", e)
        })?;

        let app = RustzxApp {
            emulator,
            snd,
            video,
            events,
            tex_border,
            tex_canvas,
            scale,
        };

        Ok(app)
    }

    pub fn start(&mut self) {
        let mut debug = false;
        let scale = self.scale;
        let mut stopwatch = InstantStopwatch::default();
        'emulator: loop {
            let frame_target_dt = frame_length(FPS);
            // absolute start time
            let frame_start = Instant::now();
            // Emulate all requested frames
            let cpu_dt = self.emulator.emulate_frames(MAX_FRAME_TIME, &mut stopwatch);
            // if sound enabled sound ganeration allowed then move samples to sound thread
            if let Some(ref mut snd) = self.snd {
                // if can be turned off even on speed change, so check it everytime
                if self.emulator.have_sound() {
                    loop {
                        if let Some(sample) = self.emulator.controller.mixer.pop() {
                            snd.send_sample(sample);
                        } else {
                            break;
                        }
                    }
                }
            }
            // load new textures to sdl
            self.video
                .update_texture(self.tex_border, self.emulator.controller.border.texture());
            self.video
                .update_texture(self.tex_canvas, self.emulator.controller.canvas.texture());
            // rendering block
            self.video.begin();
            self.video.draw_texture_2d(
                self.tex_border,
                Some(Rect::new(
                    0,
                    0,
                    SCREEN_WIDTH as u32 * scale,
                    SCREEN_HEIGHT as u32 * scale,
                )),
            );
            self.video.draw_texture_2d(
                self.tex_canvas,
                Some(Rect::new(
                    CANVAS_X as i32 * scale as i32,
                    CANVAS_Y as i32 * scale as i32,
                    CANVAS_WIDTH as u32 * scale,
                    CANVAS_HEIGHT as u32 * scale,
                )),
            );
            self.video.end();
            // check all events
            if let Some(event) = self.events.pop_event() {
                match event {
                    Event::Exit => {
                        break 'emulator;
                    }
                    Event::GameKey(key, state) => {
                        self.emulator.controller.send_key(key, state);
                    }
                    Event::SwitchDebug => {
                        debug = !debug;
                        if !debug {
                            self.video
                                .set_title(&format!("RustZX v{}", env!("CARGO_PKG_VERSION")));
                        }
                    }
                    Event::ChangeSpeed(speed) => {
                        self.emulator.set_speed(speed);
                    }
                    Event::Kempston(key, state) => {
                        if let Some(ref mut joy) = self.emulator.controller.kempston {
                            joy.key(key, state);
                        }
                    }
                    Event::InsertTape => self.emulator.controller.tape.play(),
                    Event::StopTape => self.emulator.controller.tape.stop(),
                    Event::OpenFile(_path) => {
                        // TODO: Implement Drag-n-drop file loading after global refactoring
                        // self.emulator.load_file_autodetect(path);
                    }
                }
            }
            // how long emulation iteration was
            let emulation_dt = frame_start.elapsed();
            if emulation_dt < frame_target_dt {
                let wait_koef = if self.emulator.have_sound() { 9 } else { 10 };
                // sleep untill frame sync
                thread::sleep((frame_target_dt - emulation_dt) * wait_koef / 10);
            };
            // get exceed clocks and use them on next iteration
            let frame_dt = frame_start.elapsed();
            // change window header
            if debug {
                self.video.set_title(&format!(
                    "CPU: {:7.3}ms; FRAME:{:7.3}ms",
                    cpu_dt.as_millis(),
                    frame_dt.as_millis()
                ));
            }
        }
    }
}