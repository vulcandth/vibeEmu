//! Super Game Boy host: JOYP packets, colorization, borders and controller multiplexing.
//!
//! Transfers consume the LCD's final two-bit shades, including BGP/OBP mapping,
//! scroll, window and sprites. They must not read tile RAM directly.

/// Width of the SNES output, including the SGB border.
pub const WIDTH: usize = 256;
/// Height of the visible SNES output.
pub const HEIGHT: usize = 224;

#[derive(Debug, Clone, Copy)]
enum Transfer {
    Palettes,
    Tiles(usize),
    Border,
    Attributes,
}

/// SNES-side state shared by SGB and SGB2.
#[derive(Debug)]
pub struct Sgb {
    command: [u8; 112],
    bit: usize,
    pulse: bool,
    receiving: bool,
    stop: bool,
    joyp: u8,
    enabled: bool,
    players: u8,
    player_mask: u8,
    player: u8,
    mask: u8,
    palettes: [[u16; 4]; 4],
    palette_ram: [u8; 4096],
    attributes: [u8; 360],
    attribute_files: [u8; 4050],
    tiles: [u8; 8192],
    border_map: [u16; 1024],
    border_palettes: [u16; 64],
    border_enabled: bool,
    transfer: Option<(Transfer, u8)>,
    screen: [u8; 160 * 144],
    displayed: [u8; 160 * 144],
    native_screen: Option<Vec<u32>>,
    native_displayed: Option<Vec<u32>>,
    output: Vec<u32>,
    unsupported: u32,
}

impl Default for Sgb {
    fn default() -> Self {
        Self {
            command: [0; 112],
            bit: 0,
            pulse: true,
            receiving: false,
            stop: false,
            joyp: 0x30,
            enabled: true,
            players: 1,
            player_mask: 0,
            player: 0,
            mask: 0,
            palettes: [[0x67bf, 0x265b, 0x10b5, 0x2866]; 4],
            palette_ram: [0; 4096],
            attributes: [0; 360],
            attribute_files: [0; 4050],
            tiles: [0; 8192],
            border_map: [0; 1024],
            border_palettes: [0; 64],
            border_enabled: false,
            transfer: None,
            screen: [0; 160 * 144],
            displayed: [0; 160 * 144],
            native_screen: None,
            native_displayed: None,
            output: vec![0; WIDTH * HEIGHT],
            unsupported: 0,
        }
    }
}

impl Sgb {
    /// Configure command acceptance from the SGB flag and new licensee marker.
    pub fn set_cartridge_header(&mut self, rom: &[u8]) {
        self.enabled = rom.get(0x146) == Some(&3) && rom.get(0x14b) == Some(&0x33);
    }

    /// Number of controllers requested by the game (one, two or four).
    pub fn player_count(&self) -> u8 {
        self.players
    }

    /// Zero-based controller currently selected on JOYP.
    pub fn current_player(&self) -> usize {
        usize::from(self.player)
    }

    /// Controller identification returned when neither JOYP button group is selected.
    pub fn controller_id(&self) -> u8 {
        0x0f - self.player
    }

    /// Bit mask of command IDs requiring unsupported SNES facilities.
    pub fn unsupported_commands(&self) -> u32 {
        self.unsupported
    }

    /// The complete 256 by 224 RGB output, with the GB image at (48, 40).
    pub fn framebuffer(&self) -> &[u32] {
        &self.output
    }

    /// Accept a write to the Game Boy JOYP selection lines.
    pub fn write_joyp(&mut self, value: u8) {
        let value = value & 0x30;
        if !self.enabled {
            self.joyp = value;
            return;
        }
        // The controller counter advances on P15's rising edge, even while
        // sending commands. MLT_REQ masks the counter to the new controller mode.
        if value & 0x20 != 0 && self.joyp & 0x20 == 0 {
            self.player = (self.player + 1) & self.player_mask;
        }
        self.joyp = value;
        match value {
            0x30 => self.pulse = true,
            0 if self.pulse => {
                if self.bit % 128 != 0 || self.bit == 0 || self.stop {
                    self.command.fill(0);
                    self.bit = 0;
                    self.stop = false;
                }
                self.receiving = true;
                self.pulse = false;
            }
            0x10 | 0x20 if self.pulse && self.receiving => {
                self.pulse = false;
                if self.stop {
                    self.receiving = false;
                    self.stop = false;
                    let header = self.command[0];
                    let boot_packet = header & 0xf1 == 0xf1;
                    let packets = if boot_packet {
                        1
                    } else {
                        usize::from(header & 7).max(1)
                    };
                    if value == 0x10 || self.bit == packets * 128 {
                        if value == 0x20 && !boot_packet && header & 7 != 0 {
                            self.execute();
                        }
                        self.bit = 0;
                        self.command.fill(0);
                    }
                } else if self.bit < self.command.len() * 8 {
                    if value == 0x10 {
                        self.command[self.bit / 8] |= 1 << (self.bit % 8);
                    }
                    self.bit += 1;
                    self.stop = self.bit % 128 == 0;
                }
            }
            _ => {}
        }
    }

    fn execute(&mut self) {
        let c = self.command;
        let len = usize::from(c[0] & 7) * 16;
        match c[0] >> 3 {
            id @ 0..=3 => {
                let (a, b) = [(0, 1), (2, 3), (0, 3), (1, 2)][usize::from(id)];
                for palette in &mut self.palettes {
                    palette[0] = word(&c, 1);
                }
                for n in 1..4 {
                    self.palettes[a][n] = word(&c, 1 + n * 2);
                    self.palettes[b][n] = word(&c, 7 + n * 2);
                }
            }
            4 => {
                for block in c[2..len].chunks_exact(6).take(usize::from(c[1])) {
                    let control = block[0] & 7;
                    let inside = block[1] & 3;
                    let outside = (block[1] >> 4) & 3;
                    let edge = match control {
                        1 => inside,
                        4 => outside,
                        _ => (block[1] >> 2) & 3,
                    };
                    let [left, top, right, bottom] =
                        [block[2] & 31, block[3] & 31, block[4] & 31, block[5] & 31];
                    for y in 0..18u8 {
                        for x in 0..20u8 {
                            let palette = if x < left || x > right || y < top || y > bottom {
                                (control & 4 != 0).then_some(outside)
                            } else if x == left || x == right || y == top || y == bottom {
                                (control & 2 != 0 || control == 1 || control == 4).then_some(edge)
                            } else {
                                (control & 1 != 0).then_some(inside)
                            };
                            if let Some(palette) = palette {
                                self.attributes[usize::from(y) * 20 + usize::from(x)] = palette;
                            }
                        }
                    }
                }
            }
            5 => {
                for &line in c[2..len].iter().take(usize::from(c[1])) {
                    let coordinate = usize::from(line & 31);
                    let palette = (line >> 5) & 3;
                    if line & 0x80 != 0 {
                        if coordinate < 18 {
                            self.attributes[coordinate * 20..coordinate * 20 + 20].fill(palette);
                        }
                    } else if coordinate < 20 {
                        for y in 0..18 {
                            self.attributes[y * 20 + coordinate] = palette;
                        }
                    }
                }
            }
            6 => {
                for y in 0..18 {
                    for x in 0..20 {
                        let coordinate = if c[1] & 0x40 != 0 { y } else { x };
                        self.attributes[y * 20 + x] = match coordinate.cmp(&usize::from(c[2] & 31))
                        {
                            std::cmp::Ordering::Less => (c[1] >> 2) & 3,
                            std::cmp::Ordering::Equal => (c[1] >> 4) & 3,
                            std::cmp::Ordering::Greater => c[1] & 3,
                        };
                    }
                }
            }
            7 => {
                let (mut x, mut y) = (usize::from(c[1]).min(19), usize::from(c[2]).min(17));
                let count = usize::from(word(&c, 3)).min(360).min((len - 6) * 4);
                for n in 0..count {
                    self.attributes[y * 20 + x] = (c[6 + n / 4] >> (6 - (n % 4) * 2)) & 3;
                    if c[5] & 1 != 0 {
                        y += 1;
                        if y == 18 {
                            y = 0;
                            x += 1;
                            if x == 20 {
                                break;
                            }
                        }
                    } else {
                        x += 1;
                        if x == 20 {
                            x = 0;
                            y += 1;
                            if y == 18 {
                                break;
                            }
                        }
                    }
                }
            }
            0x0a => {
                for n in 0..4 {
                    let offset = usize::from(word(&c, 1 + n * 2) & 511) * 8;
                    for color in 0..4 {
                        self.palettes[n][color] = word(&self.palette_ram, offset + color * 2);
                    }
                }
                let backdrop = self.palettes[0][0];
                for palette in &mut self.palettes {
                    palette[0] = backdrop;
                }
                if c[9] & 0x80 != 0 {
                    self.load_attributes(c[9] & 63);
                }
                if c[9] & 0x40 != 0 {
                    self.mask = 0;
                }
            }
            0x0b => self.transfer = Some((Transfer::Palettes, 3)),
            0x0e => {
                if c[1] & 4 != 0 {
                    self.enabled = false;
                }
            }
            0x11 => {
                self.players = match c[1] & 3 {
                    0 => 1,
                    1 => 2,
                    _ => 4,
                };
                // Invalid mode 2 retains bit 1 but cannot increment from zero.
                // SameSuite command_mlt_req measures this hardware quirk.
                self.player_mask = c[1] & 3;
                // Mode 2 also adds one before retaining bit 1 (SameSuite).
                if self.player_mask == 2 {
                    self.player = self.player.wrapping_add(1);
                }
                self.player &= self.player_mask;
            }
            0x13 => self.transfer = Some((Transfer::Tiles(usize::from(c[1] & 1) * 4096), 3)),
            0x14 => self.transfer = Some((Transfer::Border, 3)),
            0x15 => self.transfer = Some((Transfer::Attributes, 3)),
            0x16 => {
                self.load_attributes(c[1] & 63);
                if c[1] & 0x40 != 0 {
                    self.mask = 0;
                }
            }
            0x17 => self.mask = c[1] & 3,
            // BIOS patches and menu controls have no effect on this host implementation.
            0x0c | 0x0f | 0x19 => {}
            id => self.unsupported |= 1 << id,
        }
    }

    fn load_attributes(&mut self, file: u8) {
        if file > 44 {
            return;
        }
        for n in 0..360 {
            self.attributes[n] =
                (self.attribute_files[usize::from(file) * 90 + n / 4] >> (6 - n % 4 * 2)) & 3;
        }
    }

    pub(crate) fn capture_line(&mut self, y: usize, pixels: &mut [u32], native_color: bool) {
        if native_color {
            self.native_screen.get_or_insert_with(|| vec![0; 160 * 144]);
        } else {
            self.native_screen = None;
        }
        for (x, pixel) in pixels.iter_mut().enumerate() {
            self.screen[y * 160 + x] = (*pixel >> 24) as u8 & 3;
            *pixel &= 0xffffff;
            if let Some(screen) = &mut self.native_screen {
                screen[y * 160 + x] = *pixel;
            }
        }
    }

    pub(crate) fn finish_frame(&mut self, lcd_enabled: bool) {
        if lcd_enabled {
            if let Some((kind, remaining)) = self.transfer {
                if remaining > 1 {
                    self.transfer = Some((kind, remaining - 1));
                } else {
                    self.transfer = None;
                    let mut data = [0u8; 4096];
                    for tile in 0..256 {
                        for y in 0..8 {
                            for x in 0..8 {
                                let shade =
                                    self.screen[(tile / 20 * 8 + y) * 160 + tile % 20 * 8 + x];
                                data[tile * 16 + y * 2] |= (shade & 1) << (7 - x);
                                data[tile * 16 + y * 2 + 1] |= ((shade >> 1) & 1) << (7 - x);
                            }
                        }
                    }
                    match kind {
                        Transfer::Palettes => self.palette_ram = data,
                        Transfer::Tiles(offset) => {
                            self.tiles[offset..offset + 4096].copy_from_slice(&data)
                        }
                        Transfer::Attributes => self.attribute_files.copy_from_slice(&data[..4050]),
                        Transfer::Border => {
                            for n in 0..1024 {
                                self.border_map[n] = word(&data, n * 2);
                            }
                            for n in 0..64 {
                                self.border_palettes[n] = word(&data, 2048 + n * 2);
                            }
                            self.border_enabled = true;
                        }
                    }
                }
            }
            if self.mask != 1 {
                self.displayed = self.screen;
                self.native_displayed.clone_from(&self.native_screen);
            }
        }
        self.render();
    }

    fn render(&mut self) {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let in_game = (48..208).contains(&x) && (40..184).contains(&y);
                let mut color = self.palettes[0][0];
                if in_game {
                    let (gx, gy) = (x - 48, y - 40);
                    color = match self.mask {
                        2 => 0,
                        3 => self.palettes[0][0],
                        _ => {
                            self.palettes[usize::from(self.attributes[gy / 8 * 20 + gx / 8])]
                                [usize::from(self.displayed[gy * 160 + gx])]
                        }
                    };
                }
                let mut pixel = rgb(color);
                if in_game
                    && self.mask <= 1
                    && let Some(screen) = &self.native_displayed
                {
                    pixel = screen[(y - 40) * 160 + x - 48];
                }
                if self.border_enabled {
                    let entry = self.border_map[y / 8 * 32 + x / 8];
                    let tx = if entry & 0x4000 != 0 {
                        7 - x % 8
                    } else {
                        x % 8
                    };
                    let ty = if entry & 0x8000 != 0 {
                        7 - y % 8
                    } else {
                        y % 8
                    };
                    let tile = usize::from(entry & 255) * 32;
                    let mut index = 0;
                    for plane in 0..4 {
                        index |= ((self.tiles[tile + ty * 2 + (plane / 2) * 16 + plane % 2]
                            >> (7 - tx))
                            & 1)
                            << plane;
                    }
                    // The border layer covers the GB window even with priority bit zero.
                    // Tiles 256..1023 are not part of the SGB character allocation.
                    if index != 0 && entry & 0x300 == 0 {
                        pixel = rgb(self.border_palettes
                            [usize::from((entry >> 10) & 3) * 16 + usize::from(index)]);
                    }
                }
                self.output[y * WIDTH + x] = pixel;
            }
        }
    }
}

fn word(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn rgb(color: u16) -> u32 {
    let expand = |v: u16| u32::from((v & 31) << 3 | (v & 31) >> 2);
    (expand(color) << 16) | (expand(color >> 5) << 8) | expand(color >> 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(sgb: &mut Sgb, command: &[u8]) {
        for packet in command.chunks_exact(16) {
            sgb.write_joyp(0);
            sgb.write_joyp(0x30);
            for byte in packet {
                for bit in 0..8 {
                    sgb.write_joyp(if byte & (1 << bit) == 0 { 0x20 } else { 0x10 });
                    sgb.write_joyp(0x30);
                }
            }
            sgb.write_joyp(0x20);
            sgb.write_joyp(0x30);
        }
    }

    fn command(sgb: &mut Sgb, id: u8, parameters: &[u8]) {
        let len = (parameters.len() + 1).div_ceil(16) * 16;
        let mut bytes = vec![0; len];
        bytes[0] = id << 3 | (len / 16) as u8;
        bytes[1..parameters.len() + 1].copy_from_slice(parameters);
        send(sgb, &bytes);
    }

    // Encode a transfer as the actual 20-column LCD tile stream, rather than
    // copying data directly to host RAM. This catches row/tile ordering errors.
    fn transfer(sgb: &mut Sgb, id: u8, parameter: u8, data: &[u8; 4096]) {
        command(sgb, id, &[parameter]);
        for tile in 0..256 {
            for y in 0..8 {
                for x in 0..8 {
                    let bit = 7 - x;
                    sgb.screen[(tile / 20 * 8 + y) * 160 + tile % 20 * 8 + x] =
                        (data[tile * 16 + y * 2] >> bit & 1)
                            | (data[tile * 16 + y * 2 + 1] >> bit & 1) << 1;
                }
            }
        }
        for _ in 0..3 {
            sgb.finish_frame(true);
        }
    }

    #[test]
    fn palette_pairs_share_color_zero() {
        for (id, a, b) in [(0, 0, 1), (1, 2, 3), (2, 0, 3), (3, 1, 2)] {
            let mut sgb = Sgb::default();
            command(&mut sgb, id, &[1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0]);
            assert_eq!(sgb.palettes[a], [1, 2, 3, 4]);
            assert_eq!(sgb.palettes[b], [1, 5, 6, 7]);
            assert!(sgb.palettes.iter().all(|p| p[0] == 1));
        }
    }

    #[test]
    fn multi_packet_attributes_and_invalid_stop_recovery() {
        let mut sgb = Sgb::default();
        let mut bytes = [0; 32];
        bytes[0] = 7 << 3 | 2;
        bytes[3] = 80;
        bytes[6..].fill(0x1b);
        send(&mut sgb, &bytes[..16]);
        assert_eq!(sgb.attributes, [0; 360]); // no partial command execution
        send(&mut sgb, &bytes[16..]);
        assert_eq!(&sgb.attributes[..8], &[0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!(sgb.attributes[79], 3);
        assert_eq!(sgb.attributes[80], 0);

        // A one stop bit discards the entire command, and the next reset recovers.
        sgb.write_joyp(0);
        sgb.write_joyp(0x30);
        for _ in 0..128 {
            sgb.write_joyp(0x10);
            sgb.write_joyp(0x30);
        }
        sgb.write_joyp(0x10);
        sgb.write_joyp(0x30);
        command(&mut sgb, 0x17, &[2]);
        assert_eq!(sgb.mask, 2);
        assert_eq!(sgb.unsupported_commands(), 0);
    }

    #[test]
    fn boot_packets_zero_length_and_disabled_commands_do_not_execute() {
        let mut sgb = Sgb::default();
        for header in [0xf1, 0xf3, 0xf5, 0xf7, 0xf9, 0xfb, 0x88] {
            let mut packet = [0; 16];
            packet[0] = header;
            packet[1] = 3;
            send(&mut sgb, &packet);
        }
        assert_eq!(sgb.player_count(), 1);
        assert_eq!(sgb.unsupported_commands(), 0);
        command(&mut sgb, 0x11, &[3]);
        assert_eq!(sgb.player_count(), 4);
        command(&mut sgb, 0x0e, &[4]);
        command(&mut sgb, 0x11, &[0]);
        assert_eq!(sgb.player_count(), 4);
    }

    #[test]
    fn block_line_division_and_vertical_character_attributes() {
        let mut sgb = Sgb::default();
        command(&mut sgb, 4, &[1, 7, 0b11_10_01, 2, 3, 5, 6]);
        assert_eq!(sgb.attributes[0], 3);
        assert_eq!(sgb.attributes[3 * 20 + 2], 2);
        assert_eq!(sgb.attributes[4 * 20 + 3], 1);
        command(
            &mut sgb,
            5,
            &[4, 0x80 | (1 << 5) | 10, (2 << 5) | 12, 0xff, 0x7f],
        );
        assert_eq!(sgb.attributes[10 * 20], 1);
        assert_eq!(sgb.attributes[10 * 20 + 12], 2);
        command(&mut sgb, 6, &[0x40 | 0b11_10_01, 9]);
        assert_eq!(sgb.attributes[8 * 20], 2);
        assert_eq!(sgb.attributes[9 * 20], 3);
        assert_eq!(sgb.attributes[10 * 20], 1);
        command(&mut sgb, 7, &[0, 17, 3, 0, 1, 0b00_01_10_00]);
        assert_eq!(sgb.attributes[17 * 20], 0);
        assert_eq!(sgb.attributes[1], 1);
        assert_eq!(sgb.attributes[21], 2);
    }

    #[test]
    fn palette_and_attribute_file_transfers() {
        let mut sgb = Sgb::default();
        let mut data = [0; 4096];
        for n in 0..2048 {
            data[n * 2..n * 2 + 2].copy_from_slice(&(n as u16).to_le_bytes());
        }
        transfer(&mut sgb, 0x0b, 0, &data);
        command(&mut sgb, 0x0a, &[0, 0, 1, 0, 0, 1, 0xff, 1, 0]);
        assert_eq!(
            sgb.palettes,
            [
                [0, 1, 2, 3],
                [0, 5, 6, 7],
                [0, 1025, 1026, 1027],
                [0, 2045, 2046, 2047]
            ]
        );
        data.fill(0);
        data[44 * 90..45 * 90].fill(0x1b);
        transfer(&mut sgb, 0x15, 0, &data);
        command(&mut sgb, 0x17, &[1]);
        command(&mut sgb, 0x16, &[44 | 0x40]);
        assert_eq!(sgb.mask, 0);
        assert_eq!(&sgb.attributes[356..], &[0, 1, 2, 3]);
        command(&mut sgb, 0x16, &[63]);
        assert_eq!(&sgb.attributes[356..], &[0, 1, 2, 3]);
    }

    #[test]
    fn masks_lcd_off_and_transfer_delay() {
        let mut sgb = Sgb::default();
        sgb.screen.fill(1);
        sgb.finish_frame(true);
        let color = sgb.output[40 * WIDTH + 48];
        command(&mut sgb, 0x17, &[1]);
        sgb.screen.fill(3);
        sgb.finish_frame(true);
        assert_eq!(sgb.output[40 * WIDTH + 48], color);
        command(&mut sgb, 0x17, &[0]);
        sgb.finish_frame(false);
        assert_eq!(sgb.output[40 * WIDTH + 48], color);
        sgb.finish_frame(true);
        assert_eq!(sgb.output[40 * WIDTH + 48], rgb(sgb.palettes[0][3]));
        for (mask, expected) in [(2, 0), (3, rgb(sgb.palettes[0][0]))] {
            command(&mut sgb, 0x17, &[mask]);
            sgb.finish_frame(true);
            assert_eq!(sgb.output[40 * WIDTH + 48], expected);
        }
        command(&mut sgb, 0x0b, &[]);
        sgb.finish_frame(true);
        sgb.finish_frame(false); // stopped LCD cannot supply a transfer frame
        sgb.finish_frame(true);
        assert_eq!(sgb.palette_ram[0], 0);
        sgb.finish_frame(true);
        assert_eq!(sgb.palette_ram[0], 255);
    }

    #[test]
    fn border_planes_banks_flips_and_window_overlay() {
        let mut sgb = Sgb::default();
        let mut tiles = [0; 4096];
        tiles[0] = 0x80; // top left: color 1
        tiles[17] = 0x80; // top left: color 9
        transfer(&mut sgb, 0x13, 1, &tiles);
        let mut map = [0; 4096];
        for (n, entry) in [(0, 0x1080u16), (1, 0xd080), (5 * 32 + 6, 0x1080)] {
            map[n * 2..n * 2 + 2].copy_from_slice(&entry.to_le_bytes());
        }
        map[2048 + 9 * 2] = 0x1f; // red
        transfer(&mut sgb, 0x14, 0, &map);
        assert_eq!(sgb.output[0], 0xff0000);
        assert_eq!(sgb.output[7 * WIDTH + 15], 0xff0000);
        assert_eq!(sgb.output[40 * WIDTH + 48], 0xff0000); // priority bit is zero
        assert_ne!(sgb.output[40 * WIDTH + 49], 0xff0000); // transparent color zero
    }
}
