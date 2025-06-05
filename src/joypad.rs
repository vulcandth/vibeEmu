// src/joypad.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoypadButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

pub struct Joypad {
    action_buttons: u8,      // Bit 0: A, Bit 1: B, Bit 2: Select, Bit 3: Start (0 = pressed)
    direction_buttons: u8,   // Bit 0: Right, Bit 1: Left, Bit 2: Up, Bit 3: Down (0 = pressed)
    select_action_buttons: bool,    // True if P15 (bit 5 of P1 register) is low
    select_direction_buttons: bool, // True if P14 (bit 4 of P1 register) is low
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            action_buttons: 0x0F,      // All not pressed (bits are high)
            direction_buttons: 0x0F,   // All not pressed (bits are high)
            select_action_buttons: false,
            select_direction_buttons: false,
        }
    }

    pub fn write_p1(&mut self, value: u8) {
        // Bit 4 (0b0001_0000) selects Direction Keys (if low)
        // Bit 5 (0b0010_0000) selects Action Keys (if low)
        self.select_direction_buttons = (value & 0b0001_0000) == 0;
        self.select_action_buttons = (value & 0b0010_0000) == 0;
        // TODO: Request Joypad interrupt if conditions met (high-to-low transition on selected line's bits 0-3)
        // This is now handled by button_event returning a boolean.
        // Note: The subtask specifies that write_p1 itself does not trigger interrupts,
        // but changing selection lines while buttons are held down can also effectively cause H->L on P1's output.
        // This aspect will be handled by how button_event checks old P1 state via get_p1_lower_nibble().
    }

    // Helper to get current P1 lower nibble based on selection
    // This reflects what the CPU would read from bits 0-3 of P1 register.
    fn get_p1_lower_nibble(&self) -> u8 {
        let mut mask_value = 0x0F; // Default to all lines high (1) - if neither line selected
        if self.select_direction_buttons {
            mask_value &= self.direction_buttons;
        }
        if self.select_action_buttons {
            // If only action is selected, mask_value was 0x0F, so it becomes action_buttons.
            // If direction was also selected, mask_value is already direction_buttons, so it becomes (dir & act).
            mask_value &= self.action_buttons;
        }
        mask_value
    }

    pub fn read_p1(&self) -> u8 {
        // Bits 7-6 are always 1 (or rather, not connected, pull-up makes them 1)
        // Bits 5-4 reflect the selection lines (0 if selected, 1 if not)
        // Bits 3-0 are the button states (0 if pressed, 1 if not)
        //              or 1s if the corresponding selection line is not active.

        let mut result = 0xC0; // Start with bits 7 and 6 high, others low (will be set)

        // Set selection bits (bit 5 for action, bit 4 for direction)
        // If not selected, the bit is 1. If selected, it's 0.
        if !self.select_action_buttons {
            result |= 0b0010_0000; // Bit 5 high
        }
        if !self.select_direction_buttons {
            result |= 0b0001_0000; // Bit 4 high
        }

        let mut button_bits = 0x0F; // Default to all high (not pressed / not selected)

        if self.select_direction_buttons {
            button_bits &= self.direction_buttons;
        }

        // If action buttons are selected, AND their state with the current button_bits.
        // This handles the case where both might be selected (direction_buttons already applied if P14 was low).
        // If only action buttons are selected, button_bits is initially 0x0F, so it becomes self.action_buttons.
        if self.select_action_buttons {
             button_bits &= self.action_buttons;
        }
        // If neither is selected, button_bits remains 0x0F.

        result |= button_bits;
        result
    }

    pub fn button_event(&mut self, button: JoypadButton, pressed: bool) -> bool {
        let mut request_interrupt = false;

        // Get the state of P1's lower nibble *before* this button event changes any internal state.
        let old_p1_low_nibble = self.get_p1_lower_nibble();

        let (button_group_is_action, bit_index) = match button {
            JoypadButton::Right  => (false, 0),
            JoypadButton::Left   => (false, 1),
            JoypadButton::Up     => (false, 2),
            JoypadButton::Down   => (false, 3),
            JoypadButton::A      => (true, 0),
            JoypadButton::B      => (true, 1),
            JoypadButton::Select => (true, 2),
            JoypadButton::Start  => (true, 3),
        };

        // Update the raw hardware state of the button
        if button_group_is_action {
            if pressed {
                self.action_buttons &= !(1 << bit_index);
            } else {
                self.action_buttons |= 1 << bit_index;
            }
        } else { // Direction button
            if pressed {
                self.direction_buttons &= !(1 << bit_index);
            } else {
                self.direction_buttons |= 1 << bit_index;
            }
        }

        // Check for interrupt condition if the button was pressed
        if pressed {
            // Get the new state of P1's lower nibble *after* the button state has been updated.
            let new_p1_low_nibble = self.get_p1_lower_nibble();

            // An interrupt is triggered if the *specific bit for the pressed button* in P1's lower nibble
            // transitioned from high (1) to low (0).
            // This requires that the button's line (action/direction) is currently selected.
            let line_selected = if button_group_is_action {
                self.select_action_buttons
            } else {
                self.select_direction_buttons
            };

            if line_selected {
                // Check if the bit corresponding to the pressed button in old_p1_low_nibble was high (1)
                // AND the same bit in new_p1_low_nibble is low (0).
                // This implicitly means the line was selected both before and after, and the button itself caused the change.
                if (old_p1_low_nibble & (1 << bit_index)) != 0 && (new_p1_low_nibble & (1 << bit_index)) == 0 {
                    request_interrupt = true;
                }
            }
        }

        request_interrupt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_joypad() -> Joypad {
        Joypad::new()
    }

    #[test]
    fn test_joypad_initial_state() {
        let joypad = Joypad::new();
        assert_eq!(joypad.action_buttons, 0x0F);
        assert_eq!(joypad.direction_buttons, 0x0F);
        assert!(!joypad.select_action_buttons);
        assert!(!joypad.select_direction_buttons);
        // P1 read: bits 7-6 high, P15 high, P14 high, bits 3-0 high (0xFF)
        assert_eq!(joypad.read_p1(), 0xFF, "Initial P1 read should be 0xFF");
    }

    #[test]
    fn test_joypad_write_p1_select_lines() {
        let mut joypad = setup_joypad();

        // Select direction buttons (P14 low)
        joypad.write_p1(0b1110_1111); // Bit 4 low (0xEF -> P14=0, P15=1)
        assert!(joypad.select_direction_buttons);
        assert!(!joypad.select_action_buttons);
        // P1 read: P15 high (bit 5 = 1), P14 low (bit 4 = 0). Buttons all high.
        // Expected: 0b1100_1111 -> 0xCF (bits 7-6 high, bit 5 high, bit 4 low, button bits 0-3 high)
        // Result: 0xC0 | 0b0010_0000 (P15 high) | 0b0000_0000 (P14 low) | 0x0F (buttons) = 0b1110_1111 = 0xEF
        // Corrected expectation: 0b1101_1111 = 0xDF (if P15 not sel, P14 sel). My manual trace was wrong.
        // read_p1 logic: result = 0xC0. !sel_action -> result |= 0x20 (0xE0). !sel_dir is false. button_bits = 0x0F & dir_buttons (0x0F) = 0x0F. result |= 0x0F = 0xEF.
        assert_eq!(joypad.read_p1(), 0xEF, "P1 read after selecting direction buttons");


        // Select action buttons (P15 low)
        joypad.write_p1(0b1101_1111); // Bit 5 low (0xDF -> P14=1, P15=0)
        assert!(!joypad.select_direction_buttons);
        assert!(joypad.select_action_buttons);
        // P1 read: P15 low (bit 5 = 0), P14 high (bit 4 = 1). Buttons all high.
        // Expected: 0b1100_1111 -> 0xDF (bits 7-6 high, bit 5 low, bit 4 high, button bits 0-3 high)
        // Result: 0xC0 | 0b0000_0000 (P15 low) | 0b0001_0000 (P14 high) | 0x0F (buttons) = 0b1101_1111 = 0xDF
        assert_eq!(joypad.read_p1(), 0xDF, "P1 read after selecting action buttons");

        // Select both (unusual, but test)
        // P14=0 (sel_dir=true), P15=0 (sel_act=true)
        joypad.write_p1(0b1100_1111); // Bits 4 and 5 low (0xCF -> P14=0, P15=0)
        assert!(joypad.select_direction_buttons);
        assert!(joypad.select_action_buttons);
        // P1 read: P15 low, P14 low. Buttons all high (0x0F).
        // Result: 0xC0 | 0b0000_0000 (P15 low) | 0b0000_0000 (P14 low) | (dir_buttons & action_buttons = 0x0F) = 0b1100_1111 = 0xCF
        assert_eq!(joypad.read_p1(), 0xCF, "P1 read after selecting both lines (buttons ANDed)");


        // Select none (P14, P15 high)
        joypad.write_p1(0b1111_1111); // Bits 4 and 5 high
        assert!(!joypad.select_direction_buttons);
        assert!(!joypad.select_action_buttons);
        // Result: 0xC0 | 0b0010_0000 (P15 high) | 0b0001_0000 (P14 high) | 0x0F (buttons) = 0b1111_1111 = 0xFF
        assert_eq!(joypad.read_p1(), 0xFF, "P1 read after selecting no lines");
    }

    #[test]
    fn test_joypad_button_press_and_read() {
        let mut joypad = setup_joypad();
        let mut irq: bool;

        // 1. Press 'A' button (action line not selected)
        irq = joypad.button_event(JoypadButton::A, true);
        assert_eq!(joypad.action_buttons, 0b0000_1110, "A should be pressed internally");
        assert!(!irq, "IRQ should not be requested: A pressed, action line NOT selected");

        // 2. Select action buttons
        joypad.write_p1(0b1101_1111); // P15 low (select action), P14 high
        assert!(joypad.select_action_buttons);
        assert!(!joypad.select_direction_buttons);
        // P1 value should reflect A being pressed (0xDE)
        assert_eq!(joypad.read_p1(), 0xDE, "P1 read: A pressed, action line selected");

        // 3. Press 'A' again (it's already pressed, no H->L transition on P1 bit)
        // old_p1_low_nibble for action buttons (A=0, B=1, Sel=1, Start=1 -> 0b1110) is 0b1110. Bit 0 is 0.
        // No H->L transition for bit 0.
        irq = joypad.button_event(JoypadButton::A, true);
        assert!(!irq, "IRQ should not be requested: A re-pressed, action line selected, but no H->L P1 transition");

        // 4. Release 'A' button
        irq = joypad.button_event(JoypadButton::A, false);
        assert_eq!(joypad.action_buttons, 0x0F, "A should be released internally");
        assert!(!irq, "IRQ should not be requested: A released");
        // P1 value should reflect A being released (0xDF)
        assert_eq!(joypad.read_p1(), 0xDF, "P1 read: A released, action line selected");

        // 5. Press 'A' (H->L transition on P1.0 expected)
        // old_p1_low_nibble (all action buttons released = 0x0F). Bit 0 is 1.
        // New press makes action_buttons bit 0 = 0.
        irq = joypad.button_event(JoypadButton::A, true);
        assert!(irq, "IRQ should be requested: A pressed (H->L), action line selected");
        assert_eq!(joypad.read_p1(), 0xDE, "P1 read: A pressed again, action line selected");


        // 6. Press 'Right' (direction line not selected)
        joypad.write_p1(0b1111_1111); // Deselect all lines
        irq = joypad.button_event(JoypadButton::Right, true);
        assert_eq!(joypad.direction_buttons, 0b0000_1110);
        assert!(!irq, "IRQ should not be requested: Right pressed, direction line NOT selected");

        // 7. Select direction buttons
        joypad.write_p1(0b1110_1111); // P14 low (select direction), P15 high
        assert!(!joypad.select_action_buttons);
        assert!(joypad.select_direction_buttons);
        // P1 value should reflect Right being pressed (0xEE)
        assert_eq!(joypad.read_p1(), 0xEE, "P1 read: Right pressed, direction line selected");

        // 8. Press 'Down' (H->L transition on P1.3 expected)
        // old_p1_low_nibble for direction (Right=0, L=1, U=1, D=1 -> 0b1110). Bit 3 (Down) is 1.
        // New press makes direction_buttons bit 3 = 0.
        irq = joypad.button_event(JoypadButton::Down, true);
        assert_eq!(joypad.direction_buttons, 0b0000_0110); // Right and Down pressed
        assert!(irq, "IRQ should be requested: Down pressed (H->L), direction line selected");
        // P1 value reflects Right and Down pressed (0xE6)
        assert_eq!(joypad.read_p1(), 0xE6, "P1 read: Right and Down pressed, direction line selected");

        // 9. Release 'Right'
        irq = joypad.button_event(JoypadButton::Right, false);
        assert!(!irq, "IRQ should not be requested: Right released");
        assert_eq!(joypad.direction_buttons, 0b0000_0111); // Only Down pressed
        assert_eq!(joypad.read_p1(), 0xE7, "P1 read: Down pressed, direction line selected");

        // 10. Press 'Right' again (H->L on P1.0)
        // old_p1_low_nibble (R=1, L=1, U=1, D=0 -> 0b0111). Bit 0 is 1.
        irq = joypad.button_event(JoypadButton::Right, true);
        assert!(irq, "IRQ should be requested: Right pressed again (H->L), direction line selected");
        assert_eq!(joypad.direction_buttons, 0b0000_0110); // Right and Down pressed
        assert_eq!(joypad.read_p1(), 0xE6, "P1 read: Right and Down pressed again, direction line selected");
    }

    #[test]
    fn test_joypad_button_bit_mapping() {
        let mut joypad = setup_joypad();
        // Test direction buttons (no line selection for this test, just internal state)
        assert!(!joypad.button_event(JoypadButton::Right, true));
        assert_eq!(joypad.direction_buttons, 0b0000_1110);
        assert!(!joypad.button_event(JoypadButton::Left, true));
        assert_eq!(joypad.direction_buttons, 0b0000_1100);
        assert!(!joypad.button_event(JoypadButton::Up, true));
        assert_eq!(joypad.direction_buttons, 0b0000_1000);
        assert!(!joypad.button_event(JoypadButton::Down, true));
        assert_eq!(joypad.direction_buttons, 0b0000_0000);
        assert!(!joypad.button_event(JoypadButton::Right, false));
        assert_eq!(joypad.direction_buttons, 0b0000_0001);
        joypad.direction_buttons = 0x0F; // Reset

        // Test action buttons
        assert!(!joypad.button_event(JoypadButton::A, true));
        assert_eq!(joypad.action_buttons, 0b0000_1110);
        assert!(!joypad.button_event(JoypadButton::B, true));
        assert_eq!(joypad.action_buttons, 0b0000_1100);
        assert!(!joypad.button_event(JoypadButton::Select, true));
        assert_eq!(joypad.action_buttons, 0b0000_1000);
        assert!(!joypad.button_event(JoypadButton::Start, true));
        assert_eq!(joypad.action_buttons, 0b0000_0000);
        assert!(!joypad.button_event(JoypadButton::A, false));
        assert_eq!(joypad.action_buttons, 0b0000_0001);
    }

    #[test]
    fn test_read_p1_no_selection() {
        let mut joypad = setup_joypad();
        assert!(!joypad.button_event(JoypadButton::A, true)); // No IRQ as line not selected
        assert!(!joypad.button_event(JoypadButton::Down, true)); // No IRQ as line not selected
        // P14 and P15 are high (not selecting anything)
        joypad.write_p1(0b1111_1111); // This sets select_action_buttons and select_direction_buttons to false
        // Expect bits 0-3 to be high (0x0F), P14 high, P15 high. Bits 7-6 high. So 0xFF.
        assert_eq!(joypad.read_p1(), 0xFF, "P1 read with no selection should be 0xFF regardless of button state");
    }

    #[test]
    fn test_read_p1_both_lines_selected_unusual_case() {
        let mut joypad = setup_joypad();
        let mut irq;
        irq = joypad.button_event(JoypadButton::A, true);    // Action: 0b1110 (A pressed)
        assert!(!irq);
        irq = joypad.button_event(JoypadButton::Right, true); // Direction: 0b1110 (Right pressed)
        assert!(!irq);

        // Select both lines (P14 and P15 low)
        joypad.write_p1(0b1100_1111 & !(1 << 4) & !(1 << 5)); // sel_dir=true, sel_act=true
        assert!(joypad.select_direction_buttons);
        assert!(joypad.select_action_buttons);

        // P1 value: Both A and Right are pressed.
        // dir_buttons = 0x0E, action_buttons = 0x0E.
        // get_p1_lower_nibble() will return 0x0E & 0x0E = 0x0E.
        // read_p1() will be 0xC0 | 0x0E = 0xCE.
        assert_eq!(joypad.read_p1(), 0xCE, "P1 read with both lines selected and A/Right pressed");

        // Press B (action_buttons becomes 0b1100), keep Right pressed (direction_buttons 0b1110)
        // Old P1 lower nibble was 0x0E.
        // Button B is action, bit_index 1.
        // self.select_action_buttons is true.
        // (old_p1_low_nibble & (1 << 1)) was (0x0E & 0x02) = 0x02 (high).
        // New action_buttons = 0x0C. New direction_buttons = 0x0E.
        // New p1_low_nibble = 0x0C & 0x0E = 0x0C. Bit 1 is 0 (low).
        // So, H->L transition on bit 1.
        irq = joypad.button_event(JoypadButton::B, true); // Action: 0b1100 (A, B pressed)
        assert!(irq, "IRQ should be requested when B pressed, both lines selected, causing H->L on P1.1");
        // get_p1_lower_nibble() will return dir(0x0E) & act(0x0C) = 0x0C
        // read_p1() will be 0xC0 | 0x0C = 0xCC
        assert_eq!(joypad.read_p1(), 0xCC, "P1 read with both lines selected and A/B/Right pressed");
    }

    #[test]
    fn test_read_p1_detailed_cases() {
        let mut joypad = setup_joypad();

        // Nothing selected, Right pressed
        joypad.button_event(JoypadButton::Right, true); // Right is 0b1110 internally
        joypad.write_p1(0x30); // No selection (P14=1, P15=1)
        assert_eq!(joypad.read_p1(), 0xFF, "P1: No selection, Right pressed"); // Should read all high

        // Select Directions (P14=0, P15=1), Right pressed (0b1110)
        joypad.write_p1(0x20); // Select Directions (P15=1, P14=0) -> P1 value should be 0b..10....
        // Expected: 0b11101110 = 0xEE (P15 high, P14 low, Right low, LUD high)
        assert_eq!(joypad.read_p1(), 0xEE, "P1: Dir selected, Right pressed");

        // Select Directions, Left pressed (0b1101)
        joypad.button_event(JoypadButton::Right, false); // Release Right
        joypad.button_event(JoypadButton::Left, true);   // Press Left
        // Expected: 0b11101101 = 0xED (P15 high, P14 low, Left low, RUD high)
        assert_eq!(joypad.read_p1(), 0xED, "P1: Dir selected, Left pressed");

        // Select Actions (P14=1, P15=0), A pressed (0b1110)
        joypad.button_event(JoypadButton::Left, false); // Release Left
        joypad.button_event(JoypadButton::A, true);    // Press A
        joypad.write_p1(0x10); // Select Actions (P15=0, P14=1) -> P1 value should be 0b..01....
        // Expected: 0b11011110 = 0xDE (P15 low, P14 high, A low, B/Sel/Start high)
        assert_eq!(joypad.read_p1(), 0xDE, "P1: Act selected, A pressed");

        // Select Actions, B pressed (0b1101)
        joypad.button_event(JoypadButton::A, false);   // Release A
        joypad.button_event(JoypadButton::B, true);    // Press B
        // Expected: 0b11011101 = 0xDD (P15 low, P14 high, B low, A/Sel/Start high)
        assert_eq!(joypad.read_p1(), 0xDD, "P1: Act selected, B pressed");
    }

    #[test]
    fn test_button_event_interrupt_logic() {
        let mut joypad = setup_joypad();
        let mut irq;

        // Scenario 1: Press button when its line is NOT selected
        joypad.write_p1(0x30); // No lines selected (P14=1, P15=1)
        irq = joypad.button_event(JoypadButton::A, true);
        assert!(!irq, "Interrupt should not trigger if line is not selected");

        // Scenario 2: Press button when its line IS selected (H->L transition)
        joypad.write_p1(0x10); // Select Action buttons (P15=0)
        joypad.button_event(JoypadButton::A, false); // Ensure A is not pressed
        irq = joypad.button_event(JoypadButton::A, true); // Press A
        assert!(irq, "Interrupt should trigger for A press when action line selected");

        // Scenario 3: Press button that is ALREADY pressed (selected line)
        irq = joypad.button_event(JoypadButton::A, true); // A is already pressed
        assert!(!irq, "Interrupt should not trigger if button already pressed (no H->L)");

        // Scenario 4: Release button (selected line)
        irq = joypad.button_event(JoypadButton::A, false);
        assert!(!irq, "Interrupt should not trigger on button release");

        // Scenario 5: Select Direction, press Right
        joypad.write_p1(0x20); // Select Direction buttons (P14=0)
        joypad.button_event(JoypadButton::Right, false); // Ensure Right is not pressed
        irq = joypad.button_event(JoypadButton::Right, true);
        assert!(irq, "Interrupt should trigger for Right press when direction line selected");

        // Scenario 6: Both lines selected, press an action button (e.g. Start)
        // Initial state: Start not pressed.
        joypad.button_event(JoypadButton::Start, false);
        joypad.write_p1(0x00); // Select both P14 and P15
        assert!(joypad.select_action_buttons && joypad.select_direction_buttons);
        // old_p1_low_nibble will be 0x0F (assuming other buttons also not pressed)
        // Pressing Start (action button, bit 3)
        irq = joypad.button_event(JoypadButton::Start, true);
        // (old_p1_low_nibble & (1<<3)) = (0x0F & 0x08) = 0x08 (high)
        // new_p1_low_nibble will have bit 3 as 0. So H->L.
        assert!(irq, "Interrupt for Start when both lines selected");

        // Scenario 7: Both lines selected, press a direction button (e.g. Up)
        // Initial state: Up not pressed. Start is still pressed from previous.
        // action_buttons = 0b0111 (Start is bit 3)
        // direction_buttons = 0b1111
        joypad.button_event(JoypadButton::Up, false);
        joypad.write_p1(0x00); // Select both P14 and P15
        // old_p1_low_nibble = action_buttons (0b0111) & direction_buttons (0b1111) = 0b0111
        // Pressing Up (direction button, bit 2)
        // (old_p1_low_nibble & (1<<2)) = (0b0111 & 0x04) = 0x04 (high)
        irq = joypad.button_event(JoypadButton::Up, true);
        // new_direction_buttons = 0b1011.
        // new_p1_low_nibble = action_buttons(0b0111) & direction_buttons(0b1011) = 0b0011. Bit 2 is 0. H->L.
        assert!(irq, "Interrupt for Up when both lines selected");
    }
}
