use std::{
    env,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use pixels::{Pixels, SurfaceTexture, wgpu::Color};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

mod chip8;
mod font;

use chip8::Chip8;

struct App {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    chip8: Option<Chip8>,
    next_cpu: Instant,
    next_timer: Instant,
    next_frame: Instant,
}

impl App {
    fn new(path: &Path) -> anyhow::Result<Self> {
        let mut chip8 = Chip8::new();
        if let Err(error) = chip8.load(path) {
            eprintln!("Error loading rom: {error}");
            return Err(anyhow::anyhow!(error));
        }

        Ok(Self {
            window: None,
            pixels: None,
            chip8: Some(chip8),
            next_cpu: Instant::now() + Duration::from_secs_f64(1.0 / 600.0),
            next_timer: Instant::now() + Duration::from_secs_f64(1.0 / 60.0),
            next_frame: Instant::now() + Duration::from_secs_f64(1.0 / 60.0),
        })
    }

    fn advance_emulation(&mut self, now: Instant) {
        let cpu_step = Duration::from_secs_f64(1.0 / 600.0);
        let timer_step = Duration::from_secs_f64(1.0 / 60.0);

        let Some(chip8) = self.chip8.as_mut() else {
            return;
        };

        loop {
            if self.next_timer <= self.next_cpu {
                if self.next_timer > now {
                    break;
                }

                chip8.tick_timers();
                self.next_timer += timer_step;
            } else {
                if self.next_cpu > now {
                    break;
                }

                chip8.emulate();
                self.next_cpu += cpu_step;
            }
        }
    }
}

const WIDTH: u32 = 640;
const HEIGHT: u32 = 320;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("CHIP-8")
            .with_inner_size(winit::dpi::LogicalSize::new(WIDTH, HEIGHT));

        let window = Arc::new(event_loop.create_window(attributes).unwrap());

        let pixels = {
            let window_size = window.inner_size();
            let surface_texture = SurfaceTexture::new(
                window_size.width,
                window_size.height,
                window.clone(),
            );
            Pixels::new(64, 32, surface_texture).unwrap()
        };

        self.window = Some(window);
        self.pixels = Some(pixels);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        self.advance_emulation(now);

        if now >= self.next_frame {
            let frame_step = Duration::from_secs_f64(1.0 / 60.0);

            // skip missed frames, preserving the schedule
            while self.next_frame <= now {
                self.next_frame += frame_step;
            }

            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        let next_wakeup =
            self.next_cpu.min(self.next_timer).min(self.next_frame);

        event_loop.set_control_flow(ControlFlow::WaitUntil(next_wakeup));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                if let Some(chip8) = self.chip8.as_mut()
                    && let Some(key) = chip8_key(code)
                {
                    chip8.set_key(key, key_state == ElementState::Pressed);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(pixels) = self.pixels.as_mut() {
                    if let Err(error) =
                        pixels.resize_surface(size.width, size.height)
                    {
                        eprintln!("Error resizing surface: {error}");
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.chip8.is_none() {
                    return;
                }

                if let Some(pixels) = self.pixels.as_mut() {
                    pixels.clear_color(Color::BLACK);
                    let frame = pixels.frame_mut();

                    if let Some(chip8) = self.chip8.as_mut() {
                        let mut new_pixels = [0u8; chip8::PIXELS * 4];
                        for (index, pixel) in chip8.screen().iter().enumerate()
                        {
                            match pixel {
                                0 => (),
                                1 => {
                                    let i = index * 4;
                                    new_pixels[i] = 255;
                                    new_pixels[i + 1] = 255;
                                    new_pixels[i + 2] = 255;
                                    new_pixels[i + 3] = 255;
                                }
                                _ => (),
                            }
                        }
                        frame.copy_from_slice(&new_pixels);
                    }

                    if let Err(error) = pixels.render() {
                        eprintln!("Error rendering pixels: {error}");
                        event_loop.exit();
                    }
                }
            }
            _ => (),
        }
    }
}

fn chip8_key(code: KeyCode) -> Option<usize> {
    use KeyCode::*;

    const KEYS: [KeyCode; 16] = [
        KeyX, // 0
        Digit1, Digit2, Digit3, // 1, 2, 3
        KeyQ, KeyW, KeyE, // 4, 5, 6
        KeyA, KeyS, KeyD, // 7, 8, 9
        KeyZ, KeyC, // A, B
        Digit4, KeyR, KeyF, KeyV, // C, D, E, F
    ];

    KEYS.iter().position(|&key| key == code)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("Usage: {} <file_path>", args[0])
    }

    let path_str = &args[1];
    let path = Path::new(path_str);

    let event_loop =
        EventLoop::new().context("failed to create the event loop")?;

    let mut app = App::new(path).context("failed to create app")?;
    event_loop
        .run_app(&mut app)
        .context("event loop stopped with an error")
}
