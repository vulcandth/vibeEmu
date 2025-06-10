use minifb::{Key, MouseButton, Window, WindowOptions};

// Define window dimensions (consider making these configurable if Display::new takes them as args)
pub const WINDOW_WIDTH: usize = 160;
pub const WINDOW_HEIGHT: usize = 144;

/// Converts a PPU RGB888 framebuffer to minifb's U32 (ARGB or XRGB) buffer.
/// minifb expects ARGB format where Alpha is the highest byte.
/// If alpha is not used, it's typically set to 0xFF (opaque) or 0x00.
/// This implementation packs RGB into 0x00RRGGBB.
pub fn convert_rgb_to_u32_buffer(rgb_buffer: &[u8], width: usize, height: usize) -> Vec<u32> {
    let mut u32_buffer = vec![0u32; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx_rgb = (y * width + x) * 3;
            // Ensure we don't read out of bounds if rgb_buffer is unexpectedly short
            if idx_rgb + 2 < rgb_buffer.len() {
                let r = rgb_buffer[idx_rgb] as u32;
                let g = rgb_buffer[idx_rgb + 1] as u32;
                let b = rgb_buffer[idx_rgb + 2] as u32;
                // Format: 0x00RRGGBB (Alpha set to 00, could be FF for opaque)
                u32_buffer[y * width + x] = (r << 16) | (g << 8) | b;
            }
        }
    }
    u32_buffer
}

pub struct Display {
    window: Option<Window>, // Option to handle potential creation failure
}

impl Display {
    /// Creates a new display window.
    /// Returns `Ok(Display)` on success, or `Err(String)` if window creation fails.
    pub fn new(title: &str) -> Result<Self, String> {
        match Window::new(title, WINDOW_WIDTH, WINDOW_HEIGHT, WindowOptions::default()) {
            Ok(w) => Ok(Self { window: Some(w) }),
            Err(e) => Err(format!("Failed to create window: {}", e)),
        }
    }

    /// Returns true if the window is open and active.
    pub fn is_open(&self) -> bool {
        self.window.as_ref().map_or(false, |w| w.is_open())
    }

    /// Updates the window with the given buffer.
    /// Also handles internal event pumping for the window.
    /// Panics if the window is not initialized or if buffer update fails.
    pub fn update_with_buffer(&mut self, buffer: &[u32]) {
        if let Some(w) = self.window.as_mut() {
            w.update_with_buffer(buffer, WINDOW_WIDTH, WINDOW_HEIGHT)
                .unwrap_or_else(|e| panic!("Failed to update window buffer: {}", e));
        } else {
            // This case should ideally not be reached if Display is always created successfully
            // or if is_open() is checked before calling update.
            // However, if Display can exist in an "unopened" state, this is critical.
            // For now, assuming `new` either succeeds or the program handles the error before using Display.
            // If Display::new can return an instance even on failure (e.g. for headless mode),
            // then this needs robust handling. Given current structure, `new` returns Result,
            // so `self.window` should be `Some` if an instance of `Display` exists and is used.
        }
    }

    /// Processes window events. Call this regularly if not calling update_with_buffer.
    /// Useful when the emulator is paused but the window still needs to be responsive.
    pub fn update_events(&mut self) {
        if let Some(w) = self.window.as_mut() {
            w.update(); // Pumps events
        }
    }

    /// Checks if a specific key is currently pressed.
    pub fn is_key_down(&self, key: Key) -> bool {
        self.window.as_ref().map_or(false, |w| w.is_key_down(key))
    }

    /// Gets a list of all keys currently pressed.
    pub fn get_keys_pressed(&self, active_modifiers: minifb::KeyRepeat) -> Option<Vec<Key>> {
        self.window
            .as_ref()
            .map(|w| w.get_keys_pressed(active_modifiers))
    }

    /// Checks if a specific mouse button is currently pressed.
    pub fn get_mouse_down(&self, button: MouseButton) -> bool {
        self.window
            .as_ref()
            .map_or(false, |w| w.get_mouse_down(button))
    }

    // It might be useful to expose other minifb window methods if needed,
    // e.g., limit_update_rate, get_mouse_pos, etc.
}
