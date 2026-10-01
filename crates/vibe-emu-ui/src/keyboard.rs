//! Egui represents Shift as one modifier, without a key event or left/right side.
//! Observe winit's physical keys before forwarding every event to eframe.
use std::sync::{Arc, Mutex};
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::*,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

#[derive(Clone, Copy, Default)]
pub struct ShiftState {
    pub down: [bool; 2],
    pub presses: [u64; 2],
}

impl ShiftState {
    fn key(&mut self, code: KeyCode, down: bool, synthetic: bool) {
        let side = match code {
            KeyCode::ShiftLeft => 0,
            KeyCode::ShiftRight => 1,
            _ => return,
        };
        if down && !self.down[side] && !synthetic {
            self.presses[side] = self.presses[side].wrapping_add(1);
        }
        self.down[side] = down;
    }

    fn release(&mut self) {
        self.down = [false; 2];
    }

    pub fn newly_pressed(&self, previous: [u64; 2]) -> [bool; 2] {
        std::array::from_fn(|side| self.presses[side] != previous[side])
    }
}

pub type SharedShift = Arc<Mutex<ShiftState>>;

struct KeyboardApp<'a> {
    app: eframe::EframeWinitApplication<'a>,
    shift: SharedShift,
}

impl ApplicationHandler<eframe::UserEvent> for KeyboardApp<'_> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        self.app.resumed(el);
    }
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        self.app.new_events(el, cause);
    }
    fn user_event(&mut self, el: &ActiveEventLoop, event: eframe::UserEvent) {
        self.app.user_event(el, event);
    }
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Ok(mut shift) = self.shift.lock() {
            match &event {
                WindowEvent::KeyboardInput {
                    event,
                    is_synthetic,
                    ..
                } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        shift.key(code, event.state == ElementState::Pressed, *is_synthetic);
                    }
                }
                WindowEvent::Focused(false) | WindowEvent::Destroyed => shift.release(),
                _ => {}
            }
        }
        self.app.window_event(el, id, event);
    }
    fn device_event(&mut self, el: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.app.device_event(el, id, event);
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.app.about_to_wait(el);
    }
    fn suspended(&mut self, el: &ActiveEventLoop) {
        if let Ok(mut shift) = self.shift.lock() {
            shift.release();
        }
        self.app.suspended(el);
    }
    fn exiting(&mut self, el: &ActiveEventLoop) {
        self.app.exiting(el);
    }
    fn memory_warning(&mut self, el: &ActiveEventLoop) {
        self.app.memory_warning(el);
    }
}

pub fn run_native(
    name: &str,
    mut options: eframe::NativeOptions,
    creator: eframe::AppCreator<'_>,
    shift: SharedShift,
) -> eframe::Result {
    let mut builder = EventLoop::<eframe::UserEvent>::with_user_event();
    if let Some(hook) = options.event_loop_builder.take() {
        hook(&mut builder);
    }
    let event_loop = builder.build()?;
    let app = eframe::create_native(name, options, creator, &event_loop);
    event_loop.run_app(&mut KeyboardApp { app, shift })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_sides_release_repeat_and_focus_synthesis_are_independent() {
        let mut state = ShiftState::default();
        state.key(KeyCode::ShiftLeft, true, false);
        state.key(KeyCode::ShiftLeft, true, false);
        assert_eq!(state.down, [true, false]);
        assert_eq!(state.presses, [1, 0]);
        state.key(KeyCode::ShiftRight, true, false);
        state.key(KeyCode::ShiftLeft, false, false);
        assert_eq!(state.down, [false, true]);
        state.release();
        assert_eq!(state.down, [false, false]);
        let armed = state.presses;
        state.key(KeyCode::ShiftLeft, true, true);
        assert_eq!(state.newly_pressed(armed), [false, false]);
        state.key(KeyCode::ShiftLeft, false, false);
        state.key(KeyCode::ShiftLeft, true, false);
        assert_eq!(state.newly_pressed(armed), [true, false]);
    }
}
