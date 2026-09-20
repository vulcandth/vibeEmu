#!/usr/bin/env python3
"""Build a shared host profiling harness against a Git revision or working tree.

Old revisions without Cpu::run_for_dots use Cpu::step in the same frame loop.
Both variants keep identical ROM loading, warmup, audio, and hashing policies.
Uses the repository release settings: opt-level 3, thin LTO, one codegen unit.
Requires Python 3.12+ and rustc; the dependency-free core is compiled directly.
"""

import argparse
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", help="Git revision; omit to snapshot the working tree")
    parser.add_argument("--output", type=Path, default=Path("target/revision-benchmark"))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    args.output.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="run-", dir=args.output.resolve()))
    source = output / "source"
    core = source / "crates/vibe-emu-core"
    if args.revision:
        revision = subprocess.check_output(
            ["git", "rev-parse", "--verify", "--end-of-options", args.revision + "^{commit}"],
            cwd=repo, text=True,
        ).strip()
        archive = subprocess.check_output(["git", "archive", revision], cwd=repo)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter="data")
    else:
        revision = "working tree at " + subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=repo, text=True,
        ).strip()
        shutil.copytree(repo / "crates/vibe-emu-core/src", core / "src")
        shutil.copyfile(repo / "crates/vibe-emu-core/Cargo.toml", core / "Cargo.toml")
        (output / "working-tree.diff").write_bytes(subprocess.check_output(
            ["git", "diff", "HEAD", "--", "crates/vibe-emu-core"], cwd=repo,
        ))

    harness = (repo / "crates/vibe-emu-core/examples/profile_core.rs").read_text()
    bounded_run = "gb.cpu\n                .run_for_dots(&mut gb.mmu, (target - gb.cpu.cycles).min(4096) as u16);"
    if "pub fn run_for_dots(" not in (core / "src/cpu.rs").read_text():
        if harness.count(bounded_run) != 1:
            raise SystemExit("profiling harness changed; review the old-CPU-API adaptation")
        harness = harness.replace(bounded_run, "gb.cpu.step(&mut gb.mmu);")
        revision += "\nharness CPU API: step"
    else:
        revision += "\nharness CPU API: run_for_dots"
    harness_path = output / "profile_core.rs"
    harness_path.write_text(harness)
    (output / "revision.txt").write_text(revision + "\n")
    (output / "compiler.txt").write_bytes(subprocess.check_output(["rustc", "-vV"]))
    manifest = tomllib.loads((core / "Cargo.toml").read_text())
    env = dict(os.environ, CARGO_PKG_VERSION=manifest["package"]["version"])
    flags = ["--edition=2024", "-Copt-level=3", "-Ccodegen-units=1", "-Clto=thin"]
    library = output / "libvibe_emu_core.rlib"
    binary = output / ("profile_core.exe" if os.name == "nt" else "profile_core")
    subprocess.run([
        "rustc", *flags, "--crate-name", "vibe_emu_core", "--crate-type=rlib",
        str(core / "src/lib.rs"), "-o", str(library),
    ], env=env, check=True)
    subprocess.run([
        "rustc", *flags, "--crate-name", "profile_core", str(harness_path),
        "--extern", f"vibe_emu_core={library}", "-o", str(binary),
    ], env=env, check=True)
    print(binary, flush=True)


if __name__ == "__main__":
    main()
