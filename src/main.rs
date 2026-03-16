use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use bonnie_32::editor::Editor;
use bonnie_32::platform::renderer::Renderer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,

    editor: Editor,
    rotation: f32,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            egui_renderer: None,

            editor: Editor::new(),
            rotation: 0.0,
            last_frame: Instant::now(),
        }
    }

    fn do_frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Render 3D scene to software framebuffer
        self.editor.render_3d(dt, &mut self.rotation);

        // Upload framebuffer to GPU
        let renderer = self.renderer.as_ref().unwrap();
        let fb = self.editor.active_framebuffer();
        renderer.upload_framebuffer(&fb.pixels, fb.width as u32, fb.height as u32);
        renderer.update_viewport(fb.width as u32, fb.height as u32);

        // egui frame
        let window = self.window.as_ref().unwrap();
        let egui_state = self.egui_state.as_mut().unwrap();
        let raw_input = egui_state.take_egui_input(window);

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            self.editor.draw(ctx);
        });

        egui_state.handle_platform_output(window, full_output.platform_output);

        let primitives = self.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [renderer.config.width, renderer.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let egui_renderer = self.egui_renderer.as_mut().unwrap();
        match renderer.render(
            egui_renderer,
            &full_output.textures_delta,
            &primitives,
            &screen_descriptor,
        ) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = window.inner_size();
                self.renderer.as_mut().unwrap().resize(size);
            }
            Err(e) => eprintln!("Render error: {e:?}"),
        }

        // Tick tracker playback
        self.editor.tick(dt as f64);

        // Process editor actions after rendering
        self.editor.process_actions();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title(format!("Bonnie-32 v{VERSION}"))
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));

        let window = Arc::new(
            event_loop.create_window(window_attrs).expect("create window"),
        );

        let renderer = Renderer::new(window.clone());

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::default(),
            &window,
            None,
            None,
            Some(renderer.device.limits().max_texture_dimension_2d as usize),
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &renderer.device,
            renderer.surface_format(),
            None,
            1,
            false,
        );

        // Apply theme + load custom fonts (Lucide icons, VT323)
        bonnie_32::editor::theme::apply(&self.egui_ctx);

        self.window = Some(window);
        self.egui_state = Some(egui_state);
        self.egui_renderer = Some(egui_renderer);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(egui_state) = self.egui_state.as_mut() {
            if let Some(window) = self.window.as_ref() {
                let response = egui_state.on_window_event(window, &event);
                if response.consumed {
                    return;
                }
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => self.do_frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}
