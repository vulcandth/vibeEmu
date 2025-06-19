//! Minimal "egui ⇄ minifb" bridge.
//! No unsafe, no OpenGL – we draw straight into the same `u32` buffer
//! you already pass to `Window::update_with_buffer`.

use egui::{Event, Modifiers, PointerButton, RawInput, epaint, vec2};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window};

pub struct EguiMiniFb {
    start: std::time::Instant,
    painter: StubRenderer,
    fb: Vec<u32>,
    w: usize,
    h: usize,
}

struct StubRenderer;

impl StubRenderer {
    fn new() -> Self {
        Self
    }

    fn paint(
        &mut self,
        _width: usize,
        _height: usize,
        _pixels_per_point: f32,
        _textures: &epaint::textures::TexturesDelta,
        _meshes: &[epaint::ClippedPrimitive],
        out: &mut [u32],
    ) {
        out.fill(0);
    }
}

impl EguiMiniFb {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            start: std::time::Instant::now(),
            painter: StubRenderer::new(),
            fb: vec![0; w * h],
            w,
            h,
        }
    }

    /// Convert minifb input to egui's `RawInput`.
    pub fn raw_input(&mut self, win: &Window) -> RawInput {
        let (w, h) = win.get_size();
        self.w = w;
        self.h = h;

        let mut ri = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                vec2(w as f32, h as f32),
            )),
            time: Some(self.start.elapsed().as_secs_f64()),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };

        match win.get_mouse_pos(MouseMode::Discard) {
            Some((x, y)) => ri.events.push(Event::PointerMoved(egui::pos2(x, y))),
            None => ri.events.push(Event::PointerGone),
        }
        for &(btn, mf) in &[
            (MouseButton::Left, PointerButton::Primary),
            (MouseButton::Right, PointerButton::Secondary),
            (MouseButton::Middle, PointerButton::Middle),
        ] {
            let pressed = win.get_mouse_down(btn);
            ri.events.push(Event::PointerButton {
                pos: egui::pos2(0.0, 0.0),
                button: mf,
                pressed,
                modifiers: Modifiers::default(),
            });
        }

        if let Some((x, y)) = win.get_scroll_wheel() {
            let delta = vec2(x, y);
            if delta != egui::Vec2::ZERO {
                ri.events.push(Event::Scroll(delta));
            }
        }

        for key in win.get_keys_pressed(KeyRepeat::Yes).iter().copied() {
            if let Some(c) = key_to_char(key) {
                ri.events.push(Event::Text(c.to_string()));
            }
        }

        ri
    }

    /// Paint egui shapes into the internal RGBA buffer.
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        shapes: Vec<epaint::ClippedShape>,
        textures: &epaint::textures::TexturesDelta,
    ) {
        let meshes = ctx.tessellate(shapes, 1.0);
        self.painter
            .paint(self.w, self.h, 1.0, textures, &meshes, &mut self.fb);
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.fb
    }
}

fn key_to_char(k: Key) -> Option<char> {
    use Key::*;
    Some(match k {
        A => 'a',
        B => 'b',
        C => 'c',
        D => 'd',
        E => 'e',
        F => 'f',
        G => 'g',
        H => 'h',
        I => 'i',
        J => 'j',
        K => 'k',
        L => 'l',
        M => 'm',
        N => 'n',
        O => 'o',
        P => 'p',
        Q => 'q',
        R => 'r',
        S => 's',
        T => 't',
        U => 'u',
        V => 'v',
        W => 'w',
        X => 'x',
        Y => 'y',
        Z => 'z',
        Key0 => '0',
        Key1 => '1',
        Key2 => '2',
        Key3 => '3',
        Key4 => '4',
        Key5 => '5',
        Key6 => '6',
        Key7 => '7',
        Key8 => '8',
        Key9 => '9',
        Space => ' ',
        _ => return None,
    })
}
