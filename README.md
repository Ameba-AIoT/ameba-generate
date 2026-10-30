# ameba-generate

Project generating tool for the Ameba Rust SDK.
Generates a ready-to-compile, ready-to-package bare-metal Rust application in one command.

## Install

```bash
# crate not yet published — install from source
git clone https://github.com/Ameba-AIoT/ameba-generate.git
cd ameba-generate
cargo install --path . --locked
```

Confirm the install:

```bash
ameba-generate --version
# ameba-generate 0.1.0
```

## Usage

```bash
# Interactive TUI (recommended)
ameba-generate

# Headless (CI / scripts)
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy --headless my-app

# List available chips and features
ameba-generate list-options
```

## Quick Start

```bash
# 1. Generate a project (interactive TUI)
ameba-generate

# 2. Input project name, select chip + features, press g to generate

# 3. Build (must run from app/ subdirectory)
cd my-app/app
cargo build

# 4. Package, flash and monitor (needs `ameba-flash` on PATH, see Prerequisites)
cargo run --release -- --port <PORT>
```

## Generated Project Layout

```
my-app/
├── .git/
├── .gitignore             ← ignores /target
├── Cargo.toml             ← workspace root (members = ["app"])
├── rust-toolchain.toml    ← stable toolchain lock
└── app/                   ← application crate  (cargo build / cargo run here)
    ├── .cargo/
    │   └── config.toml    ← ARM target + linker + `ameba-flash` runner
    ├── Cargo.toml
    └── src/main.rs
```

## Prerequisites

| Tool | Install | Purpose |
|------|---------|---------|
| `rustfmt` | `rustup component add rustfmt` | auto-format after generate |
| Ameba Rust SDK | [github.com/Ameba-AIoT/ameba-rust](https://github.com/Ameba-AIoT/ameba-rust) | source for Ameba crate deps |
| `thumbv8m.main-none-eabi` target | `rustup target add thumbv8m.main-none-eabi` | cross-compile for RTL8721F |
| `ameba-flash` | `cargo install --path <rust-sdk>/ameba-flash --locked` | cargo runner: packages the ELF into `app.bin` and flashes it over UART |

## Documentation

- [docs/user-guide.md](docs/user-guide.md) — full usage guide
- [docs/extension-guide.md](docs/extension-guide.md) — how to add new ICs, IPs, or features

## Feedback

- For questions or suggestions during development, visit the [Real-AIOT Forum](https://forum.real-aiot.com/).
- For bugs or feature requests, open a [GitHub Issue](https://github.com/Ameba-AIoT/ameba-generate/issues). Please check existing issues before opening a new one.

## License

Licensed under the Apache License, Version 2.0 — see [LICENSE](LICENSE).
