#!/usr/bin/env python3
"""Generate a deterministic CGB window-raster stress ROM for profile_core.

This synthetic workload repeatedly switches the window tile map during
rendering. It measures a renderer-heavy case, not representative gameplay.
"""

import argparse
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="destination .gbc file")
    output = parser.parse_args().output
    rom = bytearray(32768)
    rom[0x100:0x103] = bytes([0xC3, 0x50, 0x01])  # JP $0150
    rom[0x134:0x140] = b"WINDOW-PROBE"
    rom[0x143] = 0x80
    program = bytes([
        0xF3, 0x31, 0xFE, 0xFF, 0xAF, 0xE0, 0x40,  # DI; SP=$FFFE; LCD off
        0x21, 0x00, 0x80, 0x06, 0x80, 0x3E, 0x33,  # HL=$8000; B=128; A=$33
        0x22, 0x3C, 0x05, 0x20, 0xFB,              # Fill tile bytes with a ramp
        0xAF, 0xE0, 0x4A, 0x3E, 40, 0xE0, 0x4B,   # WY=0; WX=40
        0x3E, 0xF3, 0xE0, 0x40, 0x21, 0x40, 0xFF,  # LCD on; HL=$FF40
        0x3E, 0xF3, 0x77, 0x3E, 0xB3, 0x77,        # Alternate only LCDC.6
        0x18, 0xF8,                                # Repeat the two writes
    ])
    rom[0x150:0x150 + len(program)] = program
    checksum = 0
    for byte in rom[0x134:0x14D]:
        checksum = (checksum - byte - 1) & 0xFF
    rom[0x14D] = checksum
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(rom)


if __name__ == "__main__":
    main()
