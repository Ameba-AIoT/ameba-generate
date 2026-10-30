# ameba-generate User Guide

`ameba-generate` is the project scaffolding tool for the Ameba Rust SDK — one
command generates a ready-to-compile, ready-to-package bare-metal application
skeleton.

---

## Table of Contents

1. [Installation](#1-installation)
2. [Quick Start](#2-quick-start)
3. [Interactive TUI Mode](#3-interactive-tui-mode)
4. [Command-Line (Headless) Mode](#4-command-line-headless-mode)
5. [Available Options](#5-available-options)
6. [Generated Project Structure](#6-generated-project-structure)
7. [Building the Generated Project](#7-building-the-generated-project)
8. [Flashing (ameba-flash runner)](#8-flashing-ameba-flash-runner)
9. [Common Errors](#9-common-errors)
10. [Prerequisites](#10-prerequisites)

---

## 1. Installation

### Install from source (current stage — the crate isn't published to crates.io yet)

```bash
git clone https://github.com/Ameba-AIoT/ameba-generate.git
cd ameba-generate
cargo install --path . --locked
```

Confirm the install:

```bash
ameba-generate --version
# ameba-generate 0.1.0
```

> **Note**: the `rustfmt` component must be installed (used to auto-format
> after generation):
> ```bash
> rustup component add rustfmt
> ```

---

## 2. Quick Start

```bash
# 1. Generate a project (interactive TUI)
ameba-generate

# 2. Enter the project name when prompted, select chip + features in the menu, press g to generate

# 3. Build (note: run from the app/ subdirectory)
cd my-app/app
cargo build

# 4. Package, flash and monitor (needs `ameba-flash` on PATH, see section 8)
cargo run --release -- --port <PORT>
```

Or generate in one shot with headless mode:

```bash
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy my-app
cd my-app/app && cargo build                              # build only
cd my-app/app && cargo run --release -- --port <PORT>    # build + package + flash
```

---

## 3. Interactive TUI Mode

Run with no arguments, or with just a project name:

```bash
ameba-generate
# or
ameba-generate my-app
```

### Flow

**Step 1**: enter a project name (if not passed as an argument)

```
Project name: my-app
```

**Step 2**: select options in the full-screen menu

```
┌─────────────────────────────────────────────────────┐
│  ameba-generate — Select options                    │
├─────────────────────────────────────────────────────┤
│ ▸ Chip                                              │
│ ►  [✓] RTL8721F                                     │
│   [✓] Embassy (async runtime)                       │
│   [ ] Wi-Fi (ameba-wifi, STA bring-up)              │
├─────────────────────────────────────────────────────┤
│  Adds embassy-executor and embassy-time for         │
│  async/await support.                               │
├─────────────────────────────────────────────────────┤
│  ↑↓ navigate   Space toggle   g generate   q quit  │
└─────────────────────────────────────────────────────┘
```

### Keybindings

| Key | Action |
|------|------|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Space` / `Enter` | Toggle the current option (select/deselect) |
| `g` or `s` | Confirm and generate the project |
| `q` or `Esc` | Cancel and quit |

### Option Rules

- **Chip options are mutually exclusive**: only one chip can be selected at a
  time (selecting another automatically deselects the current one)
- **Dependencies are handled automatically**: deselecting an option
  automatically deselects options that depend on it (e.g. deselecting embassy
  also deselects wifi)
- **Unmet dependencies**: pressing `g` shows a red error message at the
  bottom instead of exiting

---

## 4. Command-Line (Headless) Mode

For CI environments, scripted invocations, or when you don't want the TUI.

```bash
ameba-generate --sdk <SDK_PATH> -o <OPTION>... [--headless] <NAME>
```

### Arguments

| Argument | Description | Required |
|------|------|---------|
| `--sdk <PATH>` | Path to the Ameba Rust SDK directory | Yes |
| `-o <OPTION>` | Option name, repeatable | Yes (at least the chip) |
| `--headless` | Force-skip the TUI (automatic outside a TTY) | No |
| `<NAME>` | Project name / output directory name | Yes |

### Examples

```bash
# Minimal: bare metal only, no embassy
ameba-generate --sdk /path/to/rust -o rtl8721f my-bare-app

# Embassy async mode
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy my-async-app

# Embassy + Wi-Fi
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy -o wifi my-wifi-app

# Explicit --headless in a CI script
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy --headless my-app
```

### Listing all available options

```bash
ameba-generate list-options
```

Example output:
```
Available options:

[chip]  Chip
  Select the target Ameba SoC.
    -o rtl8721f (group: chip)  —  RTL8721F
  -o embassy  —  Embassy (async runtime)
     Adds embassy-executor and embassy-time for async/await support.
  -o wifi  —  Wi-Fi (ameba-wifi, STA bring-up)
     Adds the ameba-wifi dependency and brings Wi-Fi up in STA mode at startup (WHC handlers, background task, wifi_on). Connecting to an AP is left to you — see the SDK's examples/wifi-connect.
     requires: embassy

Required groups: chip
```

---

## 5. Available Options

### Chip (select exactly one)

| Option name | Chip model | Architecture |
|--------|---------|------|
| `rtl8721f` | RTL8721F | `thumbv8m.main-none-eabi` |

### Feature options

| Option name | Description | Prerequisite |
|--------|------|---------|
| `embassy` | Embassy async runtime (executor + time) | — |
| `wifi` | Wi-Fi (ameba-wifi crate): adds the dependency and brings Wi-Fi up in STA mode at startup. Connecting to an AP is left to you — follow the SDK's `examples/wifi-connect` | `embassy` |

---

## 6. Generated Project Structure

Using `rtl8721f + embassy` as an example:

```
my-app/
├── .git/                  ← auto git init
├── .gitignore             ← ignores /target
├── Cargo.toml             ← workspace root (members = ["app"])
├── rust-toolchain.toml    ← toolchain pin (stable)
└── app/                   ← application crate (run cargo build / cargo run from here)
    ├── .cargo/
    │   └── config.toml    ← ARM target, linker, rustflags, flash runner
    ├── Cargo.toml         ← dependency declarations (ameba-hal/ameba-rt/…)
    └── src/
        └── main.rs        ← application entry point
```

### Why the target configuration lives in `app/.cargo/config.toml`

`cargo` applies settings from `.cargo/config.toml` to **that directory and all
its members**. The ARM target, linker flags and flash runner are kept in
`app/.cargo/config.toml` so they apply to the `app` crate only; building from
the workspace root would use the host target instead (see
[Common Errors](#9-common-errors)).

### `app/.cargo/config.toml` contents

```toml
[build]
target = "thumbv8m.main-none-eabi"

[target.thumbv8m.main-none-eabi]
linker    = "rust-lld"
runner    = "ameba-flash flash --monitor"
rustflags = [
    "-Clink-arg=-Tameba.x",
    "-Clink-arg=--gc-sections",
    "-Clink-arg=--nmagic",
]
```

### `app/src/main.rs` (embassy mode)

```rust
#![no_std]
#![no_main]

use ameba_rt as _;
use cortex_m as _;

use ameba_hal::println;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    ameba_hal::init(ameba_hal::Config::default());
    println!("my-app running");
    loop {
        Timer::after(Duration::from_millis(1000)).await;
    }
}
```

### `app/src/main.rs` (bare-metal mode, no embassy)

```rust
#![no_std]
#![no_main]

use ameba_rt as _;
use cortex_m as _;

use ameba_hal::println;
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    ameba_hal::init(ameba_hal::Config::default());
    println!("my-app running");
    loop {
        cortex_m::asm::wfi();
    }
}
```

---

## 7. Building the Generated Project

`cargo build` must be run from the **`app/` subdirectory**, so it picks up the
ARM target configuration in `app/.cargo/config.toml`:

```bash
cd my-app/app

# Debug build (default)
cargo build

# Release build (opt-level=z, panic=abort)
cargo build --release
```

The build output is an ELF file (no extension), at:
```
target/thumbv8m.main-none-eabi/debug/my-app      # debug ELF
target/thumbv8m.main-none-eabi/release/my-app    # release ELF
```

> **Do not** run `cargo build` from the workspace root or anywhere outside
> `app/` — it won't pick up the ARM target configuration in
> `app/.cargo/config.toml` and will build for the x86 host instead, producing
> an `unknown directive .thumb_func` error.

---

## 8. Flashing (ameba-flash runner)

The generated project uses the Ameba Rust SDK's `ameba-flash` tool as its cargo
runner (`runner = "ameba-flash flash --monitor"` in `app/.cargo/config.toml`).
`cargo run` builds the ELF, then hands it to `ameba-flash`, which extracts the
loadable sections, packages `app.bin`, flashes it over UART and opens a serial
monitor. No C SDK, Python or ARM GCC toolchain is needed.

### Install `ameba-flash` (once)

```bash
cargo install --path <rust-sdk>/ameba-flash --locked
```

`<rust-sdk>` is the Ameba Rust SDK checkout passed to `--sdk` at generation
time. The boot loader, NP firmware and flash loader are embedded in the binary,
so nothing else has to be installed. Make sure `~/.cargo/bin` is on `PATH`.

### Flash and monitor

```bash
cd my-app/app
cargo run --release -- --port /dev/ttyUSB0      # or COM29 on Windows
```

The port can also come from the environment:

```bash
export RTK_SERIAL_PORT=/dev/ttyUSB0
cargo run --release
```

If the board is running normal firmware it reboots into download mode by
itself; otherwise reset it into download mode manually. Stop the monitor with
Ctrl-C.

> **Note**: the `--` before `--port` is required — it separates `cargo run`'s
> own arguments from the ones forwarded to the runner.

---

## 9. Common Errors

### No chip selected (headless mode)

```
Error: No option selected for required group 'chip'.
       Use -o <chip> where <chip> is one of: rtl8721f
```

**Fix**: add `-o rtl8721f`.

---

### wifi selected without embassy

```
Error: option 'wifi' requires 'embassy' to also be selected.
```

**Fix**: add both `-o embassy -o wifi`.

---

### SDK path doesn't exist

```
Error: SDK path does not exist: /path/to/rust
```

**Fix**: confirm `--sdk` points at the correct Ameba Rust SDK root (containing
subdirectories like `ameba-hal/`, `ameba-rt/`, etc).

---

### `--sdk` doesn't point at the Rust SDK

```
Error: '/tmp' does not look like an Ameba Rust SDK checkout (missing ameba-hal/Cargo.toml).
```

The directory exists but isn't a Rust SDK checkout. Without this check, the
error would only surface much later, at `cargo build` time in the generated
project, as a "path dependency not found" error — far from the actual cause.

**Fix**: point at the directory containing `ameba-hal/Cargo.toml`, i.e. the
Rust SDK repo root.

---

### Destination directory already exists

```
Error: destination 'my-app' already exists
```

**Fix**: remove or rename the existing directory, or choose a different
project name.

---

### `cargo build` reports `unknown directive .thumb_func`

Built from the wrong directory on an x86 host, so the ARM target
configuration wasn't picked up.

**Fix**: you must run it from the `app/` directory:
```bash
cd my-app/app && cargo build              # ✓ correct
cd my-app     && cargo build              # ✗ reads the root config, no ARM target
cargo build --manifest-path app/Cargo.toml  # ✗ doesn't read app/.cargo/config.toml
```

---

### `cargo run` reports `program not found` / `ameba-flash` is not recognised

The runner binary isn't on `PATH`.

**Fix**: install it (see [section 8](#8-flashing-ameba-flash-runner)) and make
sure `~/.cargo/bin` is on `PATH`.

---

### `cargo run` reports `required arguments were not provided: --port`

The runner needs a serial port.

**Fix**: pass `-- --port <PORT>` or set `RTK_SERIAL_PORT`.

---

## 10. Prerequisites

### For generating a project with ameba-generate

| Tool | Install | Purpose |
|------|---------|------|
| `rustfmt` | `rustup component add rustfmt` | auto-format after generation |
| Ameba Rust SDK | [github.com/Ameba-AIoT/ameba-rust](https://github.com/Ameba-AIoT/ameba-rust) | used as the `--sdk` path |

### For building and flashing the generated project

| Tool | Install | Purpose |
|------|---------|------|
| `thumbv8m.main-none-eabi` target | `rustup target add thumbv8m.main-none-eabi` | cross-compile for RTL8721F |
| `ameba-flash` | `cargo install --path <rust-sdk>/ameba-flash --locked` | cargo runner: package + UART flash + monitor |
