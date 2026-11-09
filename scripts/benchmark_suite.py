#!/usr/bin/env python3
"""Run the fixed seven-case core corpus plus synthetic window stress.

Run from the repository root after downloading the test ROM bundles. Timing
is serial, alternating between binaries via benchmark_core.py. The optional
Linux counters cover startup, warmup, hashing and draining as well as the core.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main() -> None:
    """Run reproducible workloads and preserve raw evidence and provenance."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--polished-rom", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--counters", action="store_true")
    args = parser.parse_args()
    if args.runs <= 0:
        parser.error("runs must be positive")
    repo = Path(__file__).resolve().parents[1]
    roms = repo / "crates/vibe-emu-core/test_roms"
    cases = [
        ("polished", args.polished_rom, 1800, 300, False),
        ("silent", args.polished_rom, 1800, 300, True),
        ("dmg", roms / "dmg-acid2/dmg-acid2.gb", 600, 120, False),
        ("cpu", roms / "blargg/cpu_instrs/cpu_instrs.gb", 600, 120, False),
        ("cgb", roms / "cgb-acid2/cgb-acid2.gbc", 600, 120, False),
        ("wave", roms / "blargg/cgb_sound/rom_singles/12-wave.gb",
         600, 120, False),
        ("hell", roms / "cgb-acid-hell/cgb-acid-hell.gbc", 600, 120, False),
    ]
    binaries = {key: getattr(args, key).resolve()
                for key in ("baseline", "candidate")}
    for path in [*binaries.values(), *(case[1] for case in cases)]:
        if not path.is_file():
            parser.error(f"missing input: {path}")
    # Refuse to overwrite evidence from an earlier comparison.
    args.output.mkdir(parents=True, exist_ok=False)
    window = args.output.resolve() / "window-probe.gbc"
    subprocess.run([sys.executable, str(repo / "scripts/generate_window_benchmark.py"),
                    str(window)], check=True)
    cases.append(("window", window, 600, 120, False))
    inputs = [*binaries.values(), *(case[1].resolve() for case in cases)]
    provenance = {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in inputs}
    (args.output / "sha256.json").write_text(json.dumps(provenance, indent=2))
    for name, rom, frames, warmup, silent in cases:
        command = [sys.executable, str(repo / "scripts/benchmark_core.py"),
                   str(binaries["baseline"]), str(binaries["candidate"]),
                   str(rom.resolve()), "--frames", str(frames), "--warmup",
                   str(warmup), "--runs", str(args.runs)]
        if silent:
            command.append("--silent")
        log = args.output / f"{name}.log"
        with log.open("w") as stream:
            subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT,
                           check=True)
        print(name, log.read_text().splitlines()[-1], flush=True)
        if not args.counters:
            continue
        expected = None
        for label, binary in binaries.items():
            output = subprocess.check_output([
                "perf", "stat", "-e", "instructions:u,cycles:u", "-x,", "-o",
                str(args.output / f"{name}-{label}.csv"), "--", str(binary),
                str(rom.resolve()), "600", "120",
                "silent" if silent else "audio",
            ], text=True)
            (args.output / f"{name}-{label}.signature").write_text(output)
            fields = dict(item.split("=", 1) for item in output.split())
            signature = {key: fields[key] for key in (
                "model", "frames", "warmup", "audio", "dots", "video",
                "audio_hash", "samples", "pc",
            )}
            if expected is not None and signature != expected:
                raise SystemExit(f"counter output mismatch for {name}")
            expected = signature


if __name__ == "__main__":
    main()
