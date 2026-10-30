use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Run post-generation steps on the newly created project directory:
/// 1. Format `src/**/*.rs` with `rustfmt` (via `cargo fmt`)
/// 2. Format all `*.toml` files with `taplo`
/// 3. Run `git init` if not already inside a git repository
pub fn run(project_dir: &Path) -> Result<()> {
    fmt_rust(project_dir);
    fmt_toml(project_dir)?;
    git_init(project_dir);
    Ok(())
}

// ---------------------------------------------------------------------------

fn fmt_rust(project_dir: &Path) {
    // Run inside app/ so rustup reads rust-toolchain.toml from the workspace
    // root and the ARM target config in app/.cargo/config.toml is in scope.
    let app_dir = project_dir.join("app");
    let dir = if app_dir.is_dir() {
        app_dir
    } else {
        project_dir.to_path_buf()
    };
    let status = Command::new("cargo")
        .args(["fmt"])
        .current_dir(&dir)
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("warning: cargo fmt exited with {}", s),
        Err(e) => eprintln!("warning: could not run cargo fmt: {}", e),
    }
}

fn fmt_toml(project_dir: &Path) -> Result<()> {
    // Collect all *.toml files under the project directory.
    for entry in walkdir::WalkDir::new(project_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let content = fs::read_to_string(path)?;
        let formatted = taplo::formatter::format(
            &content,
            taplo::formatter::Options {
                align_entries: true,
                reorder_keys: true,
                // Must stay false: linker flag order in `rustflags` is
                // significant (multiple -T scripts, -L/-l pairing,
                // --start-group/--end-group).
                reorder_arrays: false,
                ..Default::default()
            },
        );
        if formatted != content {
            fs::write(path, formatted)?;
        }
    }
    Ok(())
}

fn git_init(project_dir: &Path) {
    // Only initialise if we are not already inside a git repo.
    let in_git = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(project_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !in_git {
        let status = Command::new("git")
            .arg("init")
            .current_dir(project_dir)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => eprintln!("warning: git init exited with {}", s),
            Err(e) => eprintln!("warning: could not run git init: {}", e),
        }
    }
}
