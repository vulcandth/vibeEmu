# Building vibeEmu

This document provides detailed build instructions for vibeEmu on Windows, Linux, and macOS platforms.

## Prerequisites

### All Platforms

- **Rust toolchain**: Install the latest stable Rust toolchain from [rustup.rs](https://rustup.rs/)
- **Git**: For cloning the repository

### Windows

- **Visual Studio Build Tools** or **Visual Studio** with C++ build tools
  - Download from [Visual Studio Downloads](https://visualstudio.microsoft.com/downloads/)
  - Select "Desktop development with C++" workload
  - This provides the MSVC compiler that Rust's `cc` crate uses to compile the bundled `libmobile` C library

### Linux

Install the following development packages using your distribution's package manager:

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    libx11-dev \
    libgtk-3-dev \
    libasound2-dev \
    pkg-config
```

**Fedora/RHEL:**
```bash
sudo dnf install -y \
    gcc \
    gcc-c++ \
    libX11-devel \
    gtk3-devel \
    alsa-lib-devel \
    pkg-config
```

**Arch Linux:**
```bash
sudo pacman -S \
    base-devel \
    libx11 \
    gtk3 \
    alsa-lib \
    pkg-config
```

### macOS

- **Xcode Command Line Tools**: Install via `xcode-select --install`
- macOS comes with the necessary system libraries for audio and windowing

## Cloning the Repository

```bash
git clone https://github.com/vulcandth/vibeEmu.git
cd vibeEmu
```

## Building

### Standard Build

To build the entire workspace (all crates):

```bash
cargo build
```

For an optimized release build:

```bash
cargo build --release
```

### Building Individual Crates

You can build specific crates independently:

```bash
# Build only the core emulation library
cargo build -p vibe-emu-core

# Build only the UI (automatically builds core as a dependency)
cargo build -p vibe-emu-ui

# Build with optimizations
cargo build -p vibe-emu-ui --release
```

### Build Configurations

#### Without Mobile Adapter GB Support

If you don't need Mobile Adapter GB support or want to avoid the C toolchain requirement:

```bash
cargo build -p vibe-emu-ui --no-default-features
```

#### Using System libmobile

If you have `libmobile` installed on your system and prefer to use it instead of the bundled version:

```bash
cargo build -p vibe-emu-ui --no-default-features --features mobile-system
```

**Note**: This requires `libmobile` to be installed and discoverable via `pkg-config`.

## Running

After building, you can run the emulator directly:

```bash
# Development build
cargo run -p vibe-emu-ui -- path/to/rom.gb

# Release build (better performance)
cargo run -p vibe-emu-ui --release -- path/to/rom.gb
```

The release build provides significantly better emulation performance and is recommended for playing games.

## Troubleshooting

### Windows

**Issue**: Build fails with "link.exe not found" or similar C++ linker errors.

**Solution**: Ensure you have Visual Studio Build Tools installed with the "Desktop development with C++" workload. The Rust `cc` crate automatically locates MSVC, but it needs to be installed first. Restart your terminal after installation.

**Issue**: Cannot find `cl.exe` or MSVC compiler.

**Solution**: The `cc` crate usually detects MSVC automatically. If it fails, run the build from a "Developer Command Prompt" or "Developer PowerShell" that comes with Visual Studio, or ensure the MSVC tools are in your PATH.

### Linux

**Issue**: Build fails with "cannot find -lX11" or similar library errors.

**Solution**: Install the required development packages listed in the Prerequisites section.

**Issue**: Audio doesn't work or build fails with ALSA-related errors.

**Solution**: Ensure `libasound2-dev` (Ubuntu/Debian) or `alsa-lib-devel` (Fedora/RHEL) is installed.

**Issue**: GTK-related build errors for `rfd` (file dialog crate).

**Solution**: Install GTK 3 development headers: `libgtk-3-dev` on Ubuntu/Debian or `gtk3-devel` on Fedora/RHEL.

### macOS

**Issue**: Build fails with C compiler errors.

**Solution**: Install Xcode Command Line Tools: `xcode-select --install`

### General

**Issue**: Rust compiler version errors.

**Solution**: Update your Rust toolchain to the latest stable version:
```bash
rustup update stable
```

**Issue**: Out of memory during build.

**Solution**: The default debug build uses more memory. Try building in release mode or limit parallel jobs:
```bash
cargo build --release -j 2
```

## Development

### Running Tests

```bash
# Test all crates
cargo test

# Test specific crate
cargo test -p vibe-emu-core

# Run tests with output
cargo test -- --nocapture
```

### Code Formatting

Format code according to Rust style guidelines:

```bash
cargo fmt --all
```

### Linting

Run Clippy linter:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## Additional Resources

- [Rust Installation Guide](https://www.rust-lang.org/tools/install)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [vibeEmu README](README.md)
