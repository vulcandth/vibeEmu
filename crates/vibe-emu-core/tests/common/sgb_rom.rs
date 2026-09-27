// A dual-mode ROM that only uploads a border on its SGB path, like commercial
// games that return early from SGB initialization when A identifies a CGB.
pub fn initial_border_rom() -> Vec<u8> {
    fn io(code: &mut Vec<u8>, reg: u8, value: u8) {
        code.extend([0x3e, value, 0xe0, reg]);
    }
    fn packet(code: &mut Vec<u8>, id: u8, parameter: u8) {
        let mut bytes = [0; 16];
        bytes[0] = id << 3 | 1;
        bytes[1] = parameter;
        io(code, 0, 0);
        io(code, 0, 0x30);
        for byte in bytes {
            for bit in 0..8 {
                io(code, 0, if byte & (1 << bit) == 0 { 0x20 } else { 0x10 });
                io(code, 0, 0x30);
            }
        }
        io(code, 0, 0x20);
        io(code, 0, 0x30);
    }
    fn copy(code: &mut Vec<u8>, from: u16, to: u16, count: u16) {
        code.extend([
            0x21,
            from as u8,
            (from >> 8) as u8,
            0x11,
            to as u8,
            (to >> 8) as u8,
            0x01,
            count as u8,
            (count >> 8) as u8,
            0x2a,
            0x12,
            0x13,
            0x0b,
            0x78,
            0xb1,
            0x20,
            0xf8,
        ]);
    }
    fn upload(code: &mut Vec<u8>, id: u8, source: u16) {
        io(code, 0x40, 0);
        copy(code, source, 0x8000, 4096);
        copy(code, 0x6000, 0x9800, 1024);
        io(code, 0x47, 0xe4);
        io(code, 0x42, 0);
        io(code, 0x43, 0);
        io(code, 0x40, 0x91);
        packet(code, id, 0);
        for _ in 0..4 {
            // Wait for LY to leave vblank, then reach it again.
            code.extend([
                0xf0, 0x44, 0xfe, 144, 0x28, 0xfa, 0xf0, 0x44, 0xfe, 144, 0x20, 0xfa,
            ]);
        }
    }
    let mut rom = vec![0; 0x8000];
    rom[0x100..0x103].copy_from_slice(&[0xc3, 0x50, 1]);
    rom[0x143] = 0x80;
    rom[0x146] = 3;
    rom[0x14b] = 0x33;
    rom[0x147] = 0x1b;
    rom[0x149] = 2; // MBC5 + battery RAM
    let mut code = vec![
        0xfe, 0x11, 0xc2, 0x60, 1, 0x3e, 0x42, 0xea, 0, 0xc0, 0x76, 0x18, 0xfd,
    ];
    code.resize(16, 0);
    code.push(0xf3); // DI
    // Deliberately mutate donor SRAM and WRAM; neither may leak back.
    code.extend([
        0x3e, 0x0a, 0xea, 0, 0, 0x3e, 0x99, 0xea, 0, 0xa0, 0xea, 0, 0xc0,
    ]);
    packet(&mut code, 0x11, 3);
    packet(&mut code, 0x17, 2);
    upload(&mut code, 0x13, 0x4000);
    upload(&mut code, 0x14, 0x5000);
    code.extend([0x18, 0xfe]);
    assert!(code.len() < 0x4000 - 0x150);
    rom[0x150..0x150 + code.len()].copy_from_slice(&code);
    for row in 0..8 {
        rom[0x4000 + 32 + row * 2] = 0xff;
    }
    for y in 0..28 {
        for x in 0..32 {
            let tile = u16::from(!((6..26).contains(&x) && (5..23).contains(&y))) | 0x1000;
            let offset = 0x5000 + (y * 32 + x) * 2;
            rom[offset..offset + 2].copy_from_slice(&tile.to_le_bytes());
        }
    }
    rom[0x5000 + 2050..0x5000 + 2052].copy_from_slice(&0x03e0u16.to_le_bytes());
    for tile in 0..256 {
        rom[0x6000 + tile / 20 * 32 + tile % 20] = tile as u8;
    }
    rom
}
