use crate::GameScaler;
use crate::ui::vram_viewer::VramViewerWindow;
use imgui::ConfigFlags;
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use log::warn;
use pixels::Pixels;
use std::sync::Arc;
use winit::{dpi::PhysicalSize, window::Window};

/// Wrapper for each editor window
pub struct UiWindow {
    /// OS window handle
    pub win: Window,
    /// Per-window ImGui context stored suspended. Activated only while handling events/rendering.
    pub imgui: Option<imgui::SuspendedContext>,
    /// Per-window winit backend for ImGui
    pub platform: WinitPlatform,
    /// 2D framebuffer
    pub pixels: Pixels,
    /// ImGui renderer tied to this window's device
    pub renderer: Renderer,
    /// Type of window
    pub kind: WindowKind,
    /// Optional VRAM viewer state
    pub vram_viewer: Option<VramViewerWindow>,
    /// Optional custom scaler for the main game view (used to avoid UI overlap)
    pub game_scaler: Option<Arc<GameScaler>>,
    buffer_width: u32,
    buffer_height: u32,
    surface_width: u32,
    surface_height: u32,
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum WindowKind {
    Debugger,
    VramViewer,
    Watchpoints,
    Options,
    Main,
}

impl UiWindow {
    /// Create a new UiWindow with its own renderer
    pub fn new(kind: WindowKind, win: Window, pixels: Pixels, buffer_size: (u32, u32)) -> Self {
        let suspended = imgui::SuspendedContext::create();
        let mut imgui = suspended
            .activate()
            .expect("no other ImGui context should be active while creating a UiWindow");
        imgui.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE;

        let mut platform = WinitPlatform::new(&mut imgui);
        platform.attach_window(imgui.io_mut(), &win, HiDpiMode::Rounded);

        let renderer = Renderer::new(
            &mut imgui,
            pixels.device(),
            pixels.queue(),
            RendererConfig {
                texture_format: pixels.render_texture_format(),
                ..Default::default()
            },
        );

        let suspended = imgui.suspend();
        let vram_viewer = if matches!(kind, WindowKind::VramViewer) {
            Some(VramViewerWindow::new())
        } else {
            None
        };

        let game_scaler = if matches!(kind, WindowKind::Main) {
            Some(Arc::new(GameScaler::new(
                pixels.device(),
                pixels.surface_texture_format(),
            )))
        } else {
            None
        };
        Self {
            win,
            imgui: Some(suspended),
            platform,
            pixels,
            renderer,
            kind,
            vram_viewer,
            game_scaler,
            buffer_width: buffer_size.0,
            buffer_height: buffer_size.1,
            // Unknown until we've successfully called Pixels::resize_surface.
            // We intentionally start at 0 so the first redraw can force-sync.
            surface_width: 0,
            surface_height: 0,
        }
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.surface_width, self.surface_height)
    }

    pub fn buffer_size(&self) -> (u32, u32) {
        (self.buffer_width, self.buffer_height)
    }

    pub fn ensure_surface_matches_window(&mut self) {
        let size = self.win.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        if size.width != self.surface_width || size.height != self.surface_height {
            self.resize(size);
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        if let Err(err) = self.pixels.resize_surface(new_size.width, new_size.height) {
            warn!(
                "Failed to resize surface for window {:?}: {err}",
                self.win.id()
            );
        } else {
            self.surface_width = new_size.width;
            self.surface_height = new_size.height;
        }
        if let Err(err) = self
            .pixels
            .resize_buffer(self.buffer_width, self.buffer_height)
        {
            warn!(
                "Failed to resize pixel buffer for window {:?}: {err}",
                self.win.id()
            );
        }
    }
}
