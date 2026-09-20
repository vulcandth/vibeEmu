#!/usr/bin/env python3
"""Build and compare host-only core benchmark binaries with and without LLVM PGO.

This isolates code-generation effects for the dependency-free core. It does not
change Cargo's release profile or build a PGO desktop/Android/3DS frontend.
Requires `rustup component add llvm-tools`. Profiles are specific to this source
and compiler; regenerate them after changes. All generated files stay in --output.
"""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib


def run(args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rom", type=Path, help="ROM used for the final comparison")
    parser.add_argument("--train-rom", type=Path, action="append", default=[],
                        help="training ROM; repeat for a representative corpus")
    parser.add_argument("--frames", type=int, default=1800)
    parser.add_argument("--warmup", type=int, default=300)
    parser.add_argument("--training-frames", type=int, default=600)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=Path("target/pgo-benchmark"))
    args = parser.parse_args()
    if min(args.frames, args.training_frames, args.runs) <= 0 or args.warmup < 0:
        parser.error("frame/run counts must be positive and warmup nonnegative")
    roms = [rom.resolve(strict=True) for rom in (args.train_rom or [args.rom])]
    comparison_rom = args.rom.resolve(strict=True)
    repo = Path(__file__).resolve().parents[1]
    core = repo / "crates/vibe-emu-core"
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    version = subprocess.check_output(["rustc", "-vV"], text=True)
    host = next(line.split(": ", 1)[1] for line in version.splitlines() if line.startswith("host: "))
    exe = ".exe" if os.name == "nt" else ""
    profdata_tool = sysroot / "lib/rustlib" / host / "bin" / ("llvm-profdata" + exe)
    if not profdata_tool.is_file():
        parser.error("missing llvm-profdata; run `rustup component add llvm-tools`")
    args.output.mkdir(parents=True, exist_ok=True)
    # A unique directory prevents profiles from earlier source revisions mixing.
    output = Path(tempfile.mkdtemp(prefix="run-", dir=args.output.resolve()))
    print(f"Artifacts: {output}", flush=True)
    (output / "compiler.txt").write_text(version)
    (output / "training-roms.txt").write_text("\n".join(map(str, roms)) + "\n")
    manifest = tomllib.loads((core / "Cargo.toml").read_text())
    env = dict(os.environ, CARGO_PKG_VERSION=manifest["package"]["version"])
    flags = ["-Copt-level=3", "-Ccodegen-units=1", "-Clto=thin"]

    def build(name, profile_flags):
        directory = output / name
        directory.mkdir()
        library = directory / "libvibe_emu_core.rlib"
        binary = directory / ("profile_core" + exe)
        run(["rustc", "--edition=2024", "--crate-name", "vibe_emu_core", "--crate-type=rlib",
             *flags, *profile_flags, core / "src/lib.rs", "-o", library], env=env)
        run(["rustc", "--edition=2024", "--crate-name", "profile_core", *flags, *profile_flags,
             core / "examples/profile_core.rs", "--extern", f"vibe_emu_core={library}",
             "-o", binary], env=env)
        return binary

    baseline = build("control", [])
    profiles = output / "profiles"
    profiles.mkdir()
    instrumented = build("instrumented", [f"-Cprofile-generate={profiles}"])
    # Override an inherited LLVM_PROFILE_FILE so all training profiles are merged.
    training_env = dict(env, LLVM_PROFILE_FILE=str(profiles / "%m-%p.profraw"))
    with (output / "training.log").open("w") as log:
        for rom in roms:
            run([instrumented, rom, args.training_frames, args.warmup, "audio"],
                env=training_env, stdout=log)
    profile = output / "merged.profdata"
    raw_profiles = sorted(profiles.glob("*.profraw"))
    if not raw_profiles:
        raise SystemExit("instrumented runs produced no profiles")
    run([profdata_tool, "merge", "-o", profile, *raw_profiles])
    optimized = build("optimized", [f"-Cprofile-use={profile}"])
    command = [sys.executable, repo / "scripts/benchmark_core.py", baseline, optimized,
               comparison_rom, "--frames", args.frames, "--warmup", args.warmup,
               "--runs", args.runs]
    result = subprocess.run(list(map(str, command)), text=True, stdout=subprocess.PIPE)
    (output / "comparison.log").write_text(result.stdout)
    print(result.stdout, end="")
    if result.returncode:
        raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
