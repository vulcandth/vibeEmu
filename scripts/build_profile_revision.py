#!/usr/bin/env python3
"""Build a shared host profiling harness against a Git revision or working tree.

Old revisions without Cpu::run_for_dots use Cpu::step in the same frame loop.
Both variants keep identical ROM loading, warmup, audio, and hashing policies.
Uses the repository release settings: opt-level 3, thin LTO, one codegen unit.
Requires Python 3.12+ and Cargo. The harness is a separate minimal workspace,
so core dependencies are built without the frontend or core test dependencies.
"""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib


def lock_packages(path):
    """Identify external packages, ignoring workspace package dependency lists."""
    return {
        (package["name"], package["version"], package["source"], package.get("checksum"))
        for package in tomllib.loads(path.read_text())["package"]
        if "source" in package
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", help="Git revision; omit to snapshot the working tree")
    parser.add_argument("--output", type=Path, default=Path("target/revision-benchmark"))
    parser.add_argument("--offline", action="store_true", help="use only cached Cargo dependencies")
    parser.add_argument(
        "--prepare-only", action="store_true", help="snapshot sources and prepare manifests without building"
    )
    parser.add_argument(
        "--workload", choices=("core", "polished-room"), default="core",
        help="shared harness from this checkout (used for both revisions)",
    )
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
        for name in ("Cargo.toml", "Cargo.lock"):
            if (repo / name).exists():
                shutil.copyfile(repo / name, source / name)
        if (repo / "crates/vibe-emu-core/build.rs").exists():
            shutil.copyfile(repo / "crates/vibe-emu-core/build.rs", core / "build.rs")
        (output / "working-tree.diff").write_bytes(subprocess.check_output(
            ["git", "diff", "HEAD", "--", "Cargo.toml", "Cargo.lock", "crates/vibe-emu-core"], cwd=repo,
        ))

    example = "profile_polished_room.rs" if args.workload == "polished-room" else "profile_core.rs"
    harness = (repo / "crates/vibe-emu-core/examples" / example).read_text()
    revision += "\nworkload: " + args.workload
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
    compiler = subprocess.check_output([os.environ.get("RUSTC", "rustc"), "-vV"], text=True)
    (output / "compiler.txt").write_text(compiler)
    (output / "cargo.txt").write_bytes(subprocess.check_output(["cargo", "-vV"]))
    host = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    # A binary package depending on core does not build core's dev-dependencies.
    # Its own workspace also avoids resolving/building the frontend workspace.
    build = output / "build"
    build.mkdir()
    (build / "Cargo.toml").write_text('''[workspace]
resolver = "2"

[package]
name = "vibe-core-profile-revision"
version = "0.0.0"
edition = "2024"

[[bin]]
name = "profile_core"
path = "../profile_core.rs"

[dependencies]
vibe-emu-core = { path = "../source/crates/vibe-emu-core" }

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
''')
    original_lock = source / "Cargo.lock"
    if original_lock.exists():
        shutil.copyfile(original_lock, build / "Cargo.lock")
    # Include untracked source files in provenance as well as the tracked diff.
    snapshot_files = [source / "Cargo.toml", core / "Cargo.toml", harness_path]
    snapshot_files.extend(sorted((core / "src").rglob("*")))
    snapshot_files.extend([original_lock, core / "build.rs"])
    (output / "source-files.sha256").write_text("".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(output)}\n"
        for path in snapshot_files if path.is_file()
    ))
    # Record relevant overrides without leaking unrelated Cargo registry tokens.
    build_env = {
        key: value for key, value in os.environ.items()
        if key in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"}
        or key.startswith(("CARGO_BUILD_", "CARGO_PROFILE_RELEASE_"))
        or (key.startswith("CARGO_TARGET_") and key.endswith(("_RUSTFLAGS", "_LINKER")))
    }
    (output / "build-environment.json").write_text(json.dumps(build_env, indent=2) + "\n")
    cargo_args = ["--manifest-path", str(build / "Cargo.toml")]
    if args.offline:
        cargo_args.append("--offline")
    target = build / "target"
    command = [
        "cargo", "build", *cargo_args, "--release", "--locked", "--verbose",
        "--target", host, "--target-dir", str(target), "--bin", "profile_core",
    ]
    (output / "build-command.json").write_text(json.dumps(command, indent=2) + "\n")
    if args.prepare_only:
        print(output, flush=True)
        return

    # Cargo must prune the original workspace's lockfile and add the harness.
    # Resolve first, then reject any external package not pinned in the source
    # lockfile. Compilation uses --locked against that verified resolution.
    metadata = subprocess.check_output(
        ["cargo", "metadata", *cargo_args, "--format-version=1"], cwd=build,
    )
    (output / "cargo-metadata.json").write_bytes(metadata)
    if original_lock.exists():
        unexpected = lock_packages(build / "Cargo.lock") - lock_packages(original_lock)
        if unexpected:
            raise SystemExit(
                "dependency resolution changed source lockfile pins; inspect "
                f"{build / 'Cargo.lock'}: {sorted(unexpected)}"
            )
    else:
        print("Source revision has no Cargo.lock; resolved versions are saved in build/Cargo.lock.", file=sys.stderr)
    print(f"Building profiling harness; compiler commands are recorded in {output / 'build.log'}", file=sys.stderr)
    with (output / "build.log").open("w") as log:
        result = subprocess.run(command, cwd=build, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        print((output / "build.log").read_text()[-12000:], file=sys.stderr)
        raise SystemExit(result.returncode)
    binary = output / ("profile_core.exe" if os.name == "nt" else "profile_core")
    shutil.copy2(target / host / "release" / binary.name, binary)
    print(binary, flush=True)


if __name__ == "__main__":
    main()
