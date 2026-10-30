mod generator;
mod postprocess;
mod process;
mod template;
mod tui;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ameba-generate",
    version,
    about = "Project generating tool for Ameba Rust SDK"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the Ameba Rust SDK checkout (crates are not yet on crates.io)
    #[arg(long, value_name = "PATH", global = true)]
    sdk: Option<String>,

    /// Select an option (repeatable, e.g. -o rtl8721f -o embassy)
    #[arg(short = 'o', long = "option", value_name = "OPTION", global = true)]
    options: Vec<String>,

    /// Skip interactive TUI and use only -o flags (required in non-TTY / CI environments)
    #[arg(long, global = true)]
    headless: bool,

    /// Project name / output directory
    name: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// List all available options
    ListOptions,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load and parse template.yaml once.
    let tmpl = template::load()?;

    match cli.command {
        Some(Command::ListOptions) => {
            print_options(&tmpl);
            return Ok(());
        }
        None => {}
    }

    // Determine whether to use TUI or headless mode.
    // TUI: no --headless flag AND no -o options AND running in a TTY.
    let use_tui = !cli.headless
        && cli.options.is_empty()
        && cli.name.is_none()
        && std::io::stdout().is_terminal();

    let (project_name_str, sdk_raw, selected_strings) = if use_tui {
        // Interactive TUI: prompts for name, SDK path, and options.
        match tui::run(&tmpl, cli.sdk.as_deref())? {
            Some(r) => (r.project_name, r.sdk_path, r.selected),
            None => {
                println!("Cancelled.");
                return Ok(());
            }
        }
    } else {
        // Headless / scripted mode.
        let name = cli.name.as_deref().unwrap_or_default();
        if name.is_empty() {
            bail!(
                "Project name is required.\n\n\
                 Usage: ameba-generate --sdk <PATH> -o rtl8721f [-o embassy] <NAME>\n\
                        ameba-generate list-options\n\n\
                 Tip: run without arguments for interactive mode."
            );
        }
        let sdk = cli.sdk.as_deref().unwrap_or_default();
        if sdk.is_empty() {
            bail!("--sdk <PATH> is required (ameba crates are not on crates.io yet)");
        }
        (name.to_string(), sdk.to_string(), cli.options.clone())
    };

    // --- Resolve SDK path ---
    let sdk_path = PathBuf::from(&sdk_raw);
    if !sdk_path.exists() {
        bail!("SDK path does not exist: {}", sdk_path.display());
    }
    // Catch a wrong --sdk here rather than letting it surface much later as an
    // unresolvable path dependency during `cargo build` in the generated project.
    if !sdk_path.join("ameba-hal").join("Cargo.toml").is_file() {
        bail!(
            "'{}' does not look like an Ameba Rust SDK checkout \
             (missing ameba-hal/Cargo.toml).",
            sdk_path.display()
        );
    }
    let sdk_path = sdk_path.canonicalize().unwrap_or(sdk_path);
    let sdk_path = sdk_path.to_string_lossy();
    // The path is substituted verbatim into TOML and Rust string literals, so
    // on Windows it must not contain backslashes (`\\?\F:\x` is an invalid
    // escape in both).
    let sdk_path = if cfg!(windows) {
        normalize_windows_path(&sdk_path)
    } else {
        sdk_path.into_owned()
    };

    // --- Resolve destination ---
    let dest = PathBuf::from(&project_name_str);
    let project_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&project_name_str)
        .to_owned();

    // --- Validate selected options ---
    let selected_refs: Vec<&str> = selected_strings.iter().map(String::as_str).collect();
    main_validate(&tmpl, &selected_refs)?;

    if dest.exists() {
        bail!("destination '{}' already exists", dest.display());
    }
    std::fs::create_dir_all(&dest)?;

    generator::generate(&project_name, &sdk_path, &selected_refs, &dest)?;
    postprocess::run(&dest)?;

    println!("✓ Generated project '{}' (sdk: {})", project_name, sdk_path);
    println!(
        "  Selected options: {}",
        if selected_refs.is_empty() {
            "(none)".to_string()
        } else {
            selected_refs.join(", ")
        }
    );
    println!("  cd {}/app && cargo build", dest.display());
    println!(
        "  cd {}/app && cargo run --release -- --port <PORT>   # package + flash",
        dest.display()
    );

    Ok(())
}

/// Turn a canonicalized Windows path into a form that is safe to embed in a
/// quoted string: drop the `\\?\` verbatim prefix that `canonicalize()` adds
/// and use forward slashes, which Cargo and `std::path` accept on Windows.
fn normalize_windows_path(path: &str) -> String {
    let path = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    };
    path.replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Option validation — pub(crate) so tui.rs can call it
// ---------------------------------------------------------------------------

pub(crate) fn main_validate(tmpl: &template::Template, selected: &[&str]) -> Result<()> {
    let all_opts = tmpl.flat_options();

    // Check required groups have at least one selection.
    for group in &tmpl.required {
        let has_selection = all_opts.iter().any(|o| {
            o.selection_group.as_deref() == Some(group.as_str())
                && selected.contains(&o.name.as_str())
        });
        if !has_selection {
            let candidates: Vec<&str> = all_opts
                .iter()
                .filter(|o| o.selection_group.as_deref() == Some(group.as_str()))
                .map(|o| o.name.as_str())
                .collect();
            bail!(
                "No option selected for required group '{}'.\n\
                 Use -o <chip> where <chip> is one of: {}\n\
                 Run `ameba-generate list-options` for the full list.",
                group,
                candidates.join(", ")
            );
        }
    }

    // Check every selected name is a known option.
    for &name in selected {
        if !all_opts.iter().any(|o| o.name == name) {
            bail!(
                "unknown option '{}'\nRun `ameba-generate list-options` to see available options.",
                name
            );
        }
    }

    // Check mutual-exclusion groups (selection_group).
    let mut group_counts: indexmap::IndexMap<&str, Vec<&str>> = indexmap::IndexMap::new();
    for opt in &all_opts {
        if let Some(ref group) = opt.selection_group
            && selected.contains(&opt.name.as_str())
        {
            group_counts
                .entry(group.as_str())
                .or_default()
                .push(&opt.name);
        }
    }
    for (group, chosen) in &group_counts {
        if chosen.len() > 1 {
            bail!(
                "options {} belong to the same selection group '{}'; only one may be selected.",
                chosen.join(", "),
                group
            );
        }
    }

    // Check `requires` constraints.
    for opt in &all_opts {
        if selected.contains(&opt.name.as_str()) {
            for req in &opt.requires {
                if !selected.contains(&req.as_str()) {
                    bail!(
                        "option '{}' requires '{}' to also be selected.",
                        opt.name,
                        req
                    );
                }
            }
        }
    }

    // Check `compatible` constraints.
    for opt in &all_opts {
        if selected.contains(&opt.name.as_str()) {
            for (group, allowed) in &opt.compatible {
                let selected_in_group: Vec<&str> = all_opts
                    .iter()
                    .filter(|o| {
                        o.selection_group.as_deref() == Some(group.as_str())
                            && selected.contains(&o.name.as_str())
                    })
                    .map(|o| o.name.as_str())
                    .collect();
                if !selected_in_group.is_empty() {
                    let compatible = selected_in_group
                        .iter()
                        .any(|s| allowed.iter().any(|a| a == s));
                    if !compatible {
                        bail!(
                            "option '{}' is not compatible with '{}' (compatible with: {})",
                            opt.name,
                            selected_in_group.join(", "),
                            allowed.join(", ")
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// list-options
// ---------------------------------------------------------------------------

fn print_options(tmpl: &template::Template) {
    println!("Available options:\n");
    for opt in &tmpl.options {
        print_option(opt, 0);
    }
    if !tmpl.required.is_empty() {
        println!("\nRequired groups: {}", tmpl.required.join(", "));
    }
}

fn print_option(opt: &template::GeneratorOption, depth: usize) {
    let indent = "  ".repeat(depth);
    match opt {
        template::GeneratorOption::Category(cat) => {
            println!("{}[{}]  {}", indent, cat.name, cat.display_name);
            if !cat.help.is_empty() {
                println!("{}  {}", indent, cat.help);
            }
            for child in &cat.options {
                print_option(child, depth + 1);
            }
        }
        template::GeneratorOption::Option(o) => {
            let group = o
                .selection_group
                .as_deref()
                .map(|g| format!(" (group: {})", g))
                .unwrap_or_default();
            println!("{}  -o {}{}  —  {}", indent, o.name, group, o.display_name);
            if !o.help.is_empty() {
                println!("{}     {}", indent, o.help);
            }
            if !o.requires.is_empty() {
                println!("{}     requires: {}", indent, o.requires.join(", "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_windows_path;

    #[test]
    fn strips_verbatim_drive_prefix() {
        assert_eq!(
            normalize_windows_path(r"\\?\F:\hil-test\rust"),
            "F:/hil-test/rust"
        );
    }

    #[test]
    fn strips_verbatim_unc_prefix() {
        assert_eq!(
            normalize_windows_path(r"\\?\UNC\server\share\rust"),
            "//server/share/rust"
        );
    }

    #[test]
    fn plain_path_only_flips_slashes() {
        assert_eq!(
            normalize_windows_path(r"F:\hil-test\rust"),
            "F:/hil-test/rust"
        );
    }
}
