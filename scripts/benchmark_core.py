#!/usr/bin/env python3
"""Compare two profile_core binaries using alternating runs and output checksums."""

import argparse
from pathlib import Path
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("rom", type=Path)
    parser.add_argument("--frames", type=int, default=1800)
    parser.add_argument("--warmup", type=int, default=300)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--silent", action="store_true")
    args = parser.parse_args()
    if args.frames <= 0 or args.warmup < 0 or args.runs <= 0:
        parser.error("frames and runs must be positive; warmup must be nonnegative")

    times = {"baseline": [], "candidate": []}
    expected = None
    for run in range(args.runs):
        order = ["baseline", "candidate"] if run % 2 == 0 else ["candidate", "baseline"]
        for name in order:
            output = subprocess.check_output(
                [
                    str(getattr(args, name).resolve()),
                    str(args.rom.resolve()),
                    str(args.frames),
                    str(args.warmup),
                    "silent" if args.silent else "audio",
                ],
                text=True,
            ).strip()
            print(f"{name} run={run + 1} {output}", flush=True)
            fields = dict(field.split("=", 1) for field in output.split())
            signature = {key: fields[key] for key in (
                "model", "frames", "warmup", "audio", "dots", "video",
                "audio_hash", "samples", "pc",
            )}
            if expected is None:
                expected = signature
            if signature != expected:
                raise SystemExit(f"emulation output mismatch: {signature} != {expected}")
            times[name].append(float(fields["seconds"]))

    baseline = statistics.median(times["baseline"])
    candidate = statistics.median(times["candidate"])
    print(
        f"median baseline={baseline:.6f}s candidate={candidate:.6f}s "
        f"speedup={baseline / candidate:.3f}x "
        f"time_reduction={100 * (1 - candidate / baseline):.2f}%"
    )


if __name__ == "__main__":
    main()
