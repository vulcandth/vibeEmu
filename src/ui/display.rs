// Display-related utilities extracted from main.rs
use imgui::{self, Context as ImguiContext};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use pixels::Pixels;
use rfd::FileDialog;
use std::collections::HashMap;
use std::time::Duration;
use winit::dpi::PhysicalPosition;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Icon, Window, WindowBuilder};

use crate::ui::window::{UiWindow, WindowKind};
use crate::{cartridge, gameboy};

pub const SCALE: u32 = 2;
pub const GB_FPS: f64 = 59.7275;
pub const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
pub const FF_MULT: f32 = 4.0;

#[derive(Default)]
pub struct UiState {
    pub paused: bool,
    pub show_context: bool,
    pub ctx_pos: [f32; 2],
    pub spawn_debugger: bool,
    pub spawn_vram: bool,
}

pub fn load_window_icon() -> Option<Icon> {
    let icon_data = include_bytes!("../../gfx/vibeEmu_512px.png");
    if let Ok(img) = image::load_from_memory(icon_data) {
        let img = img.into_rgba8();
        let (w, h) = (img.width(), img.height());
        Icon::from_rgba(img.into_raw(), w, h).ok()
    } else {
        None
    }
}

pub fn cursor_in_screen(window: &Window, pos: PhysicalPosition<f64>) -> bool {
    let size = window.inner_size();
    let width = (160 * SCALE) as f64;
    let height = (144 * SCALE) as f64;
    let x_in = pos.x >= 0.0 && pos.x < width.min(size.width as f64);
    let y_in = pos.y >= 0.0 && pos.y < height.min(size.height as f64);
    x_in && y_in
}

pub fn spawn_debugger_window(
    event_loop: &EventLoopWindowTarget<()>,
    platform: &mut WinitPlatform,
    imgui: &mut ImguiContext,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;

    let w = WindowBuilder::new()
        .with_title("vibeEmu \u{2013} Debugger")
        .with_window_icon(load_window_icon())
        .with_inner_size(LogicalSize::new((160 * SCALE) as f64, (144 * SCALE) as f64))
        .build(event_loop)
        .unwrap();

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    let pixels = Pixels::new(1, 1, surface).expect("Pixels error");

    platform.attach_window(imgui.io_mut(), &w, HiDpiMode::Rounded);

    let ui_win = UiWindow::new(WindowKind::Debugger, w, pixels, imgui);
    windows.insert(ui_win.win.id(), ui_win);
}

pub fn spawn_vram_window(
    event_loop: &EventLoopWindowTarget<()>,
    platform: &mut WinitPlatform,
    imgui: &mut ImguiContext,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;

    let w = WindowBuilder::new()
        .with_title("vibeEmu \u{2013} VRAM")
        .with_window_icon(load_window_icon())
        .with_inner_size(LogicalSize::new((160 * SCALE) as f64, (144 * SCALE) as f64))
        .build(event_loop)
        .unwrap();

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    let pixels = Pixels::new(1, 1, surface).expect("Pixels error");

    platform.attach_window(imgui.io_mut(), &w, HiDpiMode::Rounded);

    let ui_win = UiWindow::new(WindowKind::VramViewer, w, pixels, imgui);
    windows.insert(ui_win.win.id(), ui_win);
}

pub fn draw_debugger(pixels: &mut Pixels, gb: &mut gameboy::GameBoy, ui: &imgui::Ui) {
    let _ = pixels.frame_mut();
    if let Some(_table) = ui.begin_table("regs", 2) {
        ui.table_next_row();
        ui.table_next_column();
        ui.text("A");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.a));

        ui.table_next_column();
        ui.text("F");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.f));

        ui.table_next_column();
        ui.text("B");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.b));

        ui.table_next_column();
        ui.text("C");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.c));

        ui.table_next_column();
        ui.text("D");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.d));

        ui.table_next_column();
        ui.text("E");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.e));

        ui.table_next_column();
        ui.text("H");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.h));

        ui.table_next_column();
        ui.text("L");
        ui.table_next_column();
        ui.text(format!("{:02X}", gb.cpu.l));

        ui.table_next_column();
        ui.text("SP");
        ui.table_next_column();
        ui.text(format!("{:04X}", gb.cpu.sp));

        ui.table_next_column();
        ui.text("PC");
        ui.table_next_column();
        ui.text(format!("{:04X}", gb.cpu.pc));

        ui.table_next_column();
        ui.text("IME");
        ui.table_next_column();
        ui.text(format!("{}", gb.cpu.ime));

        ui.table_next_column();
        ui.text("Cycles");
        ui.table_next_column();
        ui.text(format!("{}", gb.cpu.cycles));
    }
}

pub fn draw_vram(win: &mut UiWindow, gb: &mut gameboy::GameBoy, ui: &imgui::Ui) {
    let _ = win.pixels.frame_mut();
    if let Some(viewer) = win.vram_viewer.as_mut() {
        viewer.ui(
            ui,
            &mut gb.mmu.ppu,
            &mut win.renderer,
            win.pixels.device(),
            win.pixels.queue(),
        );
    } else {
        ui.text("VRAM viewer not initialized");
    }
}

pub fn draw_game_screen(pixels: &mut Pixels, frame: &[u32]) {
    let pixel_frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.frame_mut());
    for (dst, src) in pixel_frame.iter_mut().zip(frame) {
        let r = ((src >> 16) & 0xFF) as u8;
        let g = ((src >> 8) & 0xFF) as u8;
        let b = (src & 0xFF) as u8;
        *dst = u32::from_ne_bytes([r, g, b, 0xFF]);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_ui(
    state: &mut UiState,
    ui: &imgui::Ui,
    gb: &mut gameboy::GameBoy,
    _event_loop: &EventLoopWindowTarget<()>,
    _platform: &mut WinitPlatform,
) {
    if state.show_context {
        let flags = imgui::WindowFlags::NO_TITLE_BAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_DECORATION;
        let mut open = state.show_context;
        let mut close_menu = false;
        ui.window("ctx")
            .position(state.ctx_pos, imgui::Condition::Always)
            .flags(flags)
            .always_auto_resize(true)
            .opened(&mut open)
            .build(|| {
                if ui.button("Load ROM") {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Game Boy ROM", &["gb", "gbc"])
                        .pick_file()
                    {
                        if let Ok(cart) = cartridge::Cartridge::from_file(&path) {
                            gb.reset();
                            gb.mmu.load_cart(cart);
                            state.paused = false;
                        }
                    }
                    close_menu = true;
                }
                if ui.button("Reset GB") {
                    gb.reset();
                    state.paused = false;
                    close_menu = true;
                }
                if ui.button("Debugger") {
                    state.spawn_debugger = true;
                    close_menu = true;
                }
                if ui.button("VRAM Viewer") {
                    state.spawn_vram = true;
                    close_menu = true;
                }
            });
        state.show_context = open && !close_menu;
    }
}
