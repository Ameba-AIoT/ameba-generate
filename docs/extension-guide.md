# ameba-generate Extension Guide

This document explains how to add the following to `ameba-generate`:

- **IC (chip)**: a new Ameba SoC model
- **IP (peripheral module)**: new peripheral / protocol support (e.g. Bluetooth, Ethernet)
- **Feature**: a new compile-time option or runtime behavior toggle

---

## Architecture Overview

```
template/
├── template.yaml            # top level: declares required groups + !include of each option file
├── .template/               # option YAML metadata (not written into the generated project)
│   ├── chip.yaml            # chip category, includes rtl8721f
│   ├── embassy.yaml         # embassy async runtime option
│   └── wifi.yaml            # Wi-Fi option (requires: embassy)
├── app/
│   ├── .cargo/config.toml   # ARM target + linker
│   ├── Cargo.toml           # dependency declarations, with //IF conditional blocks
│   └── src/
│       └── main.rs          # application entry point, with //IF conditional blocks
├── Cargo.toml               # workspace root
└── rust-toolchain.toml      # toolchain pin, targets = ["{{rust_target}}"]
```

### Template Directive Quick Reference

| Directive | Location | Description |
|------|------|------|
| `#REPLACE {{placeholder}} varname` | any line in a file | Declares a placeholder→variable mapping; the line itself is not emitted, applies to all following lines |
| `//IF option("x")` | Rust / TOML comment line | Starts a conditional block, nesting supported |
| `//ELIF option("x")` | same | Conditional branch |
| `//ELSE` | same | Default branch |
| `//ENDIF` | same | Ends the conditional block |
| `//INCLUDEFILE option("x")` | first line of a file | Skips the entire file if the condition is unmet |

Condition expressions only accept `option("name")`. Getting it wrong
**errors out** rather than being silently treated as false: an unclosed
`//IF`, a stray `//ELIF`/`//ELSE`/`//ENDIF`, a condition missing its closing
paren, or an `//IF`/`//ELIF` with no condition — all of these abort
generation with the template file name and line number. Running
`cargo test` after editing a template will catch this class of issue.

### YAML Option Fields

| Field | Type | Description |
|------|------|------|
| `name` | string | Option identifier, matches the `-o` argument |
| `display_name` | string | Name shown in the TUI |
| `help` | string | Help text shown at the bottom of the TUI |
| `selection_group` | string | Mutually-exclusive group name (only one per group can be selected; chips use `chip`) |
| `requires` | list | List of prerequisite option names |
| `compatible` | map | Compatibility constraints, e.g. `chip: [rtl8721f]` |
| `sets` | map | Template variables injected when selected, e.g. `rust_target: thumbv8m.main-none-eabi` |

---

## How to Add a New IC (Chip)

Using a new chip `<new-chip>` as an example.

### Step 1: Add an option entry in `chip.yaml`

Edit `template/.template/chip.yaml` and append to the `options:` list:

```yaml
  - type: option
    name: <new-chip>
    display_name: "<New Chip Display Name>"
    selection_group: chip
    sets:
      rust_target: thumbv8m.main-none-eabi
```

`selection_group: chip` ensures it's mutually exclusive with other chips;
`sets.rust_target` injects the target triple.

### Step 2: (already done) target triple in templates

The target triple is no longer hardcoded — all three sites expand from
`sets.rust_target`, so adding a new chip needs no changes here:

```toml
# template/app/.cargo/config.toml
[build]
target = "{{rust_target}}"

[target.{{rust_target}}]
...
```

```toml
# template/rust-toolchain.toml
targets = ["{{rust_target}}"]
```

`{{var}}` substitution applies to every template line
(`src/process.rs`'s `substitute_owned`), and works the same in `.toml` and
`.rs` files — no extra `#REPLACE` needed.

### Step 3: Check the flash runner

`app/.cargo/config.toml` sets `runner = "ameba-flash flash --monitor"`. If the
new chip needs a different image layout or flash protocol, `ameba-flash` (in the
Ameba Rust SDK) has to support it first; the template itself carries no
packaging logic.

### Step 4: Update `template/app/src/main.rs` (if the chip needs its own init)

If the new chip's HAL init signature differs, wrap the differing part in a
`//IF option("<new-chip>")` block:

```rust
//IF option("<new-chip>")
ameba_hal::init_variant(ameba_hal::Config::default());
//ELSE
ameba_hal::init(ameba_hal::Config::default());
//ENDIF
```

### Verification

```bash
# rebuild ameba-generate
cargo install --path . --locked

# confirm the new chip shows up in the list
ameba-generate list-options

# generate and build
ameba-generate --sdk /path/to/rust -o <new-chip> my-new-chip-app
cd my-new-chip-app/app && cargo build
```

---

## How to Add a New IP (Peripheral Module)

Using **Bluetooth (BLE)** as an example, with `requires: embassy`.

### Step 1: Create the option YAML

Create `template/.template/ble.yaml`:

```yaml
type: option
name: ble
display_name: "BLE"
help: "Enables ameba-ble crate for Bluetooth Low Energy."
requires:
  - embassy
compatible:
  chip:
    - rtl8721f
```

### Step 2: Include it in `template.yaml`

Edit `template/template.yaml`:

```yaml
required:
  - chip

options:
  - !include .template/chip.yaml
  - !include .template/embassy.yaml
  - !include .template/wifi.yaml
  - !include .template/ble.yaml    # ← new
```

### Step 3: Add the conditional dependency in `template/app/Cargo.toml`

```toml
//IF option("ble")
ameba-ble = { path = "{{sdk-path}}/ameba-ble" }
//ENDIF
```

### Step 4: Create `template/app/src/ble.rs`

Add an `//INCLUDEFILE` guard as the first line, so the whole file is skipped
when `ble` isn't selected:

```rust
//INCLUDEFILE option("ble")
// BLE initialization — generated when the ble option is selected.

pub fn init() {
    // TODO: configure and start ameba-ble
}
```

### Step 5: Reference it in `template/app/src/main.rs`

```rust
//IF option("ble")
mod ble;
//ENDIF

// ... inside main() ...
//IF option("ble")
    ble::init();
//ENDIF
```

### Verification

```bash
cargo install --path . --locked

ameba-generate list-options
# -o ble  —  BLE (requires: embassy)  ✓

# should error when embassy isn't selected
ameba-generate --sdk /path/to/rust -o rtl8721f -o ble my-ble-app
# Error: option 'ble' requires 'embassy' to also be selected.  ✓

# normal generation
ameba-generate --sdk /path/to/rust -o rtl8721f -o embassy -o ble my-ble-app
grep ameba-ble my-ble-app/app/Cargo.toml   # ✓
ls my-ble-app/app/src/                     # main.rs  ble.rs  ✓
```

---

## How to Add a Feature (Compile Switch)

A feature is an option that doesn't depend on an external crate and only
affects code-generation behavior — for example "enable log output".

### Step 1: Create the option YAML

Create `template/.template/log.yaml`:

```yaml
type: option
name: log
display_name: "DiagPrintf logging"
help: "Enables LOGUART log output via ameba-rom-sys DiagPrintf."
```

No `requires`, no `compatible` — usable with any chip, with or without
embassy.

### Step 2: Include it in `template.yaml`

Same as above — append `- !include .template/log.yaml` to
`template/template.yaml`.

### Step 3: Add a conditional block in the template

For example, inside the loop body in `template/app/src/main.rs`:

```rust
    loop {
//IF option("log")
        unsafe { ameba_rom_sys::DiagPrintf(b"tick\n\0".as_ptr()) };
//ENDIF
        Timer::after(Duration::from_millis(1000)).await;
    }
```

No new file needed, no Cargo.toml change — just insert an `//IF` block into
an existing template file.

---

## Common Errors

| Error | Cause | Fix |
|------|------|------|
| `unknown option 'xxx'` | The option YAML wasn't included in `template.yaml` | Append its `!include` to `template.yaml` |
| `option 'xxx' requires 'yyy'` | The `requires` field is enforced and the dependency wasn't selected | Also pass `-o yyy` |
| The generated files are missing the new IP's source file | The `//INCLUDEFILE` condition is wrong, or the file isn't under `template/app/src/` | Check the directive on the first line and the file path |
| The newly generated chip fails to compile with a target error | `chip.yaml` is missing `sets.rust_target`, so `{{rust_target}}` expands to an empty string | Add `sets.rust_target` for the new chip |
| `unclosed //IF opened at line N` / `stray //ENDIF` | A conditional block is malformed | Fix it using the file name + line number from the error |
| `malformed condition "..."` | The condition isn't in `option("name")` form (commonly a missing closing paren or quote) | Rewrite it as `option("name")` |
| `ameba-generate list-options` doesn't show the new option | Forgot to `cargo install --path . --locked` again | Reinstall and retry |
