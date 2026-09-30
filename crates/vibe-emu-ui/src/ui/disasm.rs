/// Decode an SM83 instruction from the given memory slice.
/// `mem` should be a slice starting at the instruction to decode.
/// `addr` is the absolute address (used for relative jump target display).
/// Returns (mnemonic, instruction_length, optional_target_address).
/// The target address is set for JP, JR, CALL, LD with address operands.
pub fn decode_sm83(mem: &[u8], addr: u16) -> (String, u16, Option<u16>) {
    let get = |offset: usize| -> u8 { mem.get(offset).copied().unwrap_or(0) };
    let op = get(0);
    let imm8 = || get(1);
    let imm16 = || {
        let lo = get(1) as u16;
        let hi = get(2) as u16;
        (hi << 8) | lo
    };

    if op == 0xCB {
        let (s, len) = decode_cb(get(1));
        return (s, len, None);
    }

    decode_base(addr, op, imm8, imm16)
}

fn decode_base<F8, F16>(addr: u16, op: u8, imm8: F8, imm16: F16) -> (String, u16, Option<u16>)
where
    F8: Fn() -> u8,
    F16: Fn() -> u16,
{
    let x = op >> 6;
    let y = (op >> 3) & 0x07;
    let z = op & 0x07;
    let p = y >> 1;
    let q = y & 0x01;

    let r = |idx: u8| -> &'static str {
        match idx {
            0 => "b",
            1 => "c",
            2 => "d",
            3 => "e",
            4 => "h",
            5 => "l",
            6 => "[hl]",
            7 => "a",
            _ => "?",
        }
    };

    let rp = |idx: u8| -> &'static str {
        match idx {
            0 => "bc",
            1 => "de",
            2 => "hl",
            3 => "sp",
            _ => "?",
        }
    };

    let rp2 = |idx: u8| -> &'static str {
        match idx {
            0 => "bc",
            1 => "de",
            2 => "hl",
            3 => "af",
            _ => "?",
        }
    };

    let alu = |idx: u8| -> &'static str {
        match idx {
            0 => "add",
            1 => "adc",
            2 => "sub",
            3 => "sbc",
            4 => "and",
            5 => "xor",
            6 => "or",
            7 => "cp",
            _ => "?",
        }
    };

    // Relative jump: returns (mnemonic_with_placeholder, len, target)
    let rel = |mn: &str| -> (String, u16, Option<u16>) {
        let e = imm8() as i8;
        let dest = addr.wrapping_add(2).wrapping_add(e as u16);
        (format!("{mn} ${dest:04X}"), 2, Some(dest))
    };

    match x {
        0 => match z {
            0 => match y {
                0 => ("nop".to_string(), 1, None),
                1 => (format!("ld [${:04X}], sp", imm16()), 3, None),
                2 => ("stop".to_string(), 2, None),
                3 => rel("jr"),
                4 => rel("jr nz,"),
                5 => rel("jr z,"),
                6 => rel("jr nc,"),
                7 => rel("jr c,"),
                _ => unreachable!(),
            },
            1 => {
                let rp_name = rp(p);
                if q == 0 {
                    (format!("ld {rp_name}, ${:04X}", imm16()), 3, None)
                } else {
                    (format!("add hl, {rp_name}"), 1, None)
                }
            }
            2 => {
                let s = match (q, p) {
                    (0, 0) => "ld [bc], a".to_string(),
                    (0, 1) => "ld [de], a".to_string(),
                    (0, 2) => "ld [hli], a".to_string(),
                    (0, 3) => "ld [hld], a".to_string(),
                    (1, 0) => "ld a, [bc]".to_string(),
                    (1, 1) => "ld a, [de]".to_string(),
                    (1, 2) => "ld a, [hli]".to_string(),
                    (1, 3) => "ld a, [hld]".to_string(),
                    _ => format!("db ${op:02X}"),
                };
                (s, 1, None)
            }
            3 => {
                let rp_name = rp(p);
                if q == 0 {
                    (format!("inc {rp_name}"), 1, None)
                } else {
                    (format!("dec {rp_name}"), 1, None)
                }
            }
            4 => (format!("inc {}", r(y)), 1, None),
            5 => (format!("dec {}", r(y)), 1, None),
            6 => (format!("ld {}, ${:02X}", r(y), imm8()), 2, None),
            7 => match y {
                0 => ("rlca".to_string(), 1, None),
                1 => ("rrca".to_string(), 1, None),
                2 => ("rla".to_string(), 1, None),
                3 => ("rra".to_string(), 1, None),
                4 => ("daa".to_string(), 1, None),
                5 => ("cpl".to_string(), 1, None),
                6 => ("scf".to_string(), 1, None),
                7 => ("ccf".to_string(), 1, None),
                _ => (format!("db ${op:02X}"), 1, None),
            },
            _ => (format!("db ${op:02X}"), 1, None),
        },
        1 => {
            if op == 0x76 {
                return ("halt".to_string(), 1, None);
            }
            (format!("ld {}, {}", r(y), r(z)), 1, None)
        }
        2 => (format!("{} {}", alu(y), r(z)), 1, None),
        3 => match z {
            0 => match y {
                0 => ("ret nz".to_string(), 1, None),
                1 => ("ret z".to_string(), 1, None),
                2 => ("ret nc".to_string(), 1, None),
                3 => ("ret c".to_string(), 1, None),
                4 => {
                    let offset = imm8();
                    let target = 0xFF00 | (offset as u16);
                    (format!("ldh [${target:04X}], a"), 2, Some(target))
                }
                5 => {
                    let e = imm8() as i8;
                    (format!("add sp, {e}"), 2, None)
                }
                6 => {
                    let offset = imm8();
                    let target = 0xFF00 | (offset as u16);
                    (format!("ldh a, [${target:04X}]"), 2, Some(target))
                }
                7 => {
                    let e = imm8() as i8;
                    (
                        format!(
                            "ld hl, sp {} {}",
                            if e < 0 { "-" } else { "+" },
                            e.unsigned_abs()
                        ),
                        2,
                        None,
                    )
                }
                _ => (format!("db ${op:02X}"), 1, None),
            },
            1 => {
                if q == 0 {
                    (format!("pop {}", rp2(p)), 1, None)
                } else {
                    match p {
                        0 => ("ret".to_string(), 1, None),
                        1 => ("reti".to_string(), 1, None),
                        2 => ("jp hl".to_string(), 1, None),
                        3 => ("ld sp, hl".to_string(), 1, None),
                        _ => (format!("db ${op:02X}"), 1, None),
                    }
                }
            }
            2 => match y {
                0 => {
                    let target = imm16();
                    (format!("jp nz, ${target:04X}"), 3, Some(target))
                }
                1 => {
                    let target = imm16();
                    (format!("jp z, ${target:04X}"), 3, Some(target))
                }
                2 => {
                    let target = imm16();
                    (format!("jp nc, ${target:04X}"), 3, Some(target))
                }
                3 => {
                    let target = imm16();
                    (format!("jp c, ${target:04X}"), 3, Some(target))
                }
                4 => ("ldh [c], a".to_string(), 1, None),
                5 => (format!("ld [${:04X}], a", imm16()), 3, None),
                6 => ("ldh a, [c]".to_string(), 1, None),
                7 => (format!("ld a, [${:04X}]", imm16()), 3, None),
                _ => (format!("db ${op:02X}"), 1, None),
            },
            3 => match y {
                0 => {
                    let target = imm16();
                    (format!("jp ${target:04X}"), 3, Some(target))
                }
                1 => ("prefix cb".to_string(), 1, None),
                6 => ("di".to_string(), 1, None),
                7 => ("ei".to_string(), 1, None),
                _ => (format!("db ${op:02X}"), 1, None),
            },
            4 => match y {
                0 => {
                    let target = imm16();
                    (format!("call nz, ${target:04X}"), 3, Some(target))
                }
                1 => {
                    let target = imm16();
                    (format!("call z, ${target:04X}"), 3, Some(target))
                }
                2 => {
                    let target = imm16();
                    (format!("call nc, ${target:04X}"), 3, Some(target))
                }
                3 => {
                    let target = imm16();
                    (format!("call c, ${target:04X}"), 3, Some(target))
                }
                _ => (format!("db ${op:02X}"), 1, None),
            },
            5 => {
                if q == 0 {
                    (format!("push {}", rp2(p)), 1, None)
                } else if p == 0 {
                    let target = imm16();
                    (format!("call ${target:04X}"), 3, Some(target))
                } else {
                    (format!("db ${op:02X}"), 1, None)
                }
            }
            6 => (format!("{} ${:02X}", alu(y), imm8()), 2, None),
            7 => (format!("rst ${:02X}", y * 8), 1, None),
            _ => (format!("db ${op:02X}"), 1, None),
        },
        _ => (format!("db ${op:02X}"), 1, None),
    }
}

fn decode_cb(op: u8) -> (String, u16) {
    let x = op >> 6;
    let y = (op >> 3) & 0x07;
    let z = op & 0x07;

    let r = |idx: u8| -> &'static str {
        match idx {
            0 => "b",
            1 => "c",
            2 => "d",
            3 => "e",
            4 => "h",
            5 => "l",
            6 => "[hl]",
            7 => "a",
            _ => "?",
        }
    };

    let rot = |idx: u8| -> &'static str {
        match idx {
            0 => "rlc",
            1 => "rrc",
            2 => "rl",
            3 => "rr",
            4 => "sla",
            5 => "sra",
            6 => "swap",
            7 => "srl",
            _ => "?",
        }
    };

    let s = match x {
        0 => format!("{} {}", rot(y), r(z)),
        1 => format!("bit {y}, {}", r(z)),
        2 => format!("res {y}, {}", r(z)),
        3 => format!("set {y}, {}", r(z)),
        _ => format!("db $cb{op:02X}"),
    };

    (s, 2)
}

pub fn format_bytes(mem: &[u8], addr: u16, len: u16) -> String {
    let mut s = String::with_capacity(len as usize * 3);
    for i in 0..len {
        if i > 0 {
            s.push(' ');
        }
        let b = mem.get(addr as usize + i as usize).copied().unwrap_or(0);
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Index the visible memory image, preserving known entry points even when a
/// linear interpretation of preceding data would skip over them.
pub fn instruction_addresses(mem: &[u8], anchors: &[u16]) -> Vec<u16> {
    let mut result = Vec::new();
    let mut addr = 0usize;
    while addr < mem.len().min(0x10000) {
        result.push(addr as u16);
        let mut next = addr + super::code_data::sm83_instr_len(mem[addr]) as usize;
        for &anchor in anchors {
            let anchor = anchor as usize;
            if addr < anchor && anchor < next {
                next = anchor;
            }
        }
        addr = next;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polishedcrystal_dialect() {
        for (bytes, expected) in [
            ([0x32, 0, 0], "ld [hld], a"),
            ([0x2a, 0, 0], "ld a, [hli]"),
            ([0xe9, 0, 0], "jp hl"),
            ([0xf8, 0xff, 0], "ld hl, sp - 1"),
            ([0xf8, 0, 0], "ld hl, sp + 0"),
            ([0xcb, 0x7e, 0], "bit 7, [hl]"),
            ([0xea, 0x34, 0x12], "ld [$1234], a"),
            ([0x20, 0xfe, 0], "jr nz, $0100"),
        ] {
            assert_eq!(decode_sm83(&bytes, 0x100).0, expected);
        }
    }

    #[test]
    fn all_opcode_lengths_agree_with_index() {
        for opcode in 0..=255 {
            assert_eq!(
                decode_sm83(&[opcode, 0, 0], 0).1,
                u16::from(super::super::code_data::sm83_instr_len(opcode)),
                "{opcode:02X}"
            );
        }
    }

    #[test]
    fn long_jump_and_last_address_are_always_indexed() {
        let mem = vec![0xcd; 0x10000];
        let rows = instruction_addresses(&mem, &[0x8000, 0xffff]);
        assert!(rows.contains(&0x8000));
        assert!(rows.contains(&0xffff));
        assert!(rows.windows(2).all(|r| r[0] < r[1]));
    }
}
