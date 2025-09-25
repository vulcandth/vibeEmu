use crate::ui::vram_viewer::VramViewerWindow;
use imgui_wgpu::{Renderer, RendererConfig};
use pixels::Pixels;
use winit::window::Window;

/// Wrapper for each editor window
pub struct UiWindow {
    /// OS window handle
    pub win: Window,
    /// 2D framebuffer
    pub pixels: Pixels,
    /// ImGui renderer tied to this window's device
    pub renderer: Renderer,
    /// Type of window
    pub kind: WindowKind,
    /// Optional VRAM viewer state
    pub vram_viewer: Option<VramViewerWindow>,
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum WindowKind {
    Debugger,
    VramViewer,
    Main,
}

/// Update the surface and buffer size of a [`Pixels`] instance.
pub fn resize_pixels(pixels: &mut Pixels, new_size: winit::dpi::PhysicalSize<u32>) {
    // Update surface size for wgpu
    pixels.resize_surface(new_size.width, new_size.height).ok();

    // Resize backing buffer if using a fixed internal resolution
    // pixels.resize_buffer(SCREEN_W as u32, SCREEN_H as u32);
}

impl UiWindow {
    /// Create a new UiWindow with its own renderer
    pub fn new(kind: WindowKind, win: Window, pixels: Pixels, imgui: &mut imgui::Context) -> Self {
        let renderer = Renderer::new(
            imgui,
            pixels.device(),
            pixels.queue(),
            RendererConfig {
                texture_format: pixels.render_texture_format(),
                ..Default::default()
            },
        );
        let vram_viewer = if matches!(kind, WindowKind::VramViewer) {
            Some(VramViewerWindow::new())
        } else {
            None
        };
        Self {
            win,
            pixels,
            renderer,
            kind,
            vram_viewer,
        }
    }
}
