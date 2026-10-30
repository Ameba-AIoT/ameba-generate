use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Top-level template.yaml structure.
#[derive(Debug, Deserialize)]
pub struct Template {
    /// Option group names that must have at least one selection before
    /// generation can proceed (e.g. `["chip"]`).
    #[serde(default)]
    pub required: Vec<String>,

    /// Flat list of options/categories after !include expansion.
    #[serde(default)]
    pub options: Vec<GeneratorOption>,
}

/// A single selectable option or a named group of options.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GeneratorOption {
    /// A mutually-exclusive group containing child options.
    Category(Category),
    /// A leaf option the user can select.
    Option(Opt),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub options: Vec<GeneratorOption>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Opt {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub help: String,
    /// Mutual-exclusion group name (e.g. `"chip"`).
    #[serde(default)]
    pub selection_group: Option<String>,
    /// Other option names that must also be selected.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Compatibility constraints: `chip: [rtl8721f]`
    /// means this option is only valid when one of those chip options is selected.
    #[serde(default)]
    pub compatible: indexmap::IndexMap<String, Vec<String>>,
    /// Key/value pairs injected as template variables when this option is selected.
    /// Used from Step 7 onward (chip-specific target triple etc.).
    #[serde(default)]
    #[allow(dead_code)]
    pub sets: indexmap::IndexMap<String, String>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

// Generator metadata files embedded by build.rs (TEMPLATE_META_FILES array).
// These are template.yaml and .template/*.yaml — not written to the project.
include!(concat!(env!("OUT_DIR"), "/template_meta_files.rs"));

/// Parse template.yaml (with `!include` expansion) from the embedded files.
pub fn load() -> Result<Template> {
    let root_src = TEMPLATE_META_FILES
        .iter()
        .find(|(p, _)| *p == "template.yaml")
        .map(|(_, c)| *c)
        .context("template.yaml not found in embedded metadata files")?;

    // Expand !include tags before deserialising.
    let expanded = expand_includes(root_src, "template.yaml", &mut HashSet::new())?;
    let t: Template = serde_yaml::from_str(&expanded).context("failed to parse template.yaml")?;
    Ok(t)
}

/// Recursively replace `!include <path>` lines with the referenced file's
/// content. `base` is the path of the file currently being expanded (used
/// to resolve relative includes). `seen` detects cycles.
fn expand_includes(src: &str, base: &str, seen: &mut HashSet<String>) -> Result<String> {
    if seen.contains(base) {
        bail!("include cycle detected: {}", base);
    }
    seen.insert(base.to_string());

    // Compute the directory of `base` so we can resolve relative paths.
    let base_dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or("");

    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(path) = trimmed
            .strip_prefix("- !include ")
            .or_else(|| trimmed.strip_prefix("!include "))
        {
            let path = path.trim();
            // Resolve relative to parent directory.
            let resolved = if base_dir.is_empty() {
                path.to_string()
            } else {
                format!("{}/{}", base_dir, path)
            };

            let included_src = TEMPLATE_META_FILES
                .iter()
                .find(|(p, _)| *p == resolved)
                .map(|(_, c)| *c)
                .with_context(|| format!("!include target not found: {}", resolved))?;

            // Indent the included content to match the `- !include` indentation.
            let indent = &line[..line.len() - trimmed.len()];

            let expanded = expand_includes(included_src, &resolved, seen)?;
            // Wrap as a list item if the original line started with `- `
            if trimmed.starts_with("- ") {
                // First line gets `- `, subsequent lines get two extra spaces of indent.
                let mut first = true;
                for inc_line in expanded.lines() {
                    if first {
                        out.push_str(&format!("{}- {}\n", indent, inc_line));
                        first = false;
                    } else {
                        out.push_str(&format!("{}  {}\n", indent, inc_line));
                    }
                }
            } else {
                for inc_line in expanded.lines() {
                    out.push_str(&format!("{}{}\n", indent, inc_line));
                }
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    seen.remove(base);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Flat iteration helpers
// ---------------------------------------------------------------------------

impl Template {
    /// Iterate over all leaf `Opt` nodes in the option tree.
    pub fn flat_options(&self) -> Vec<&Opt> {
        let mut result = Vec::new();
        for o in &self.options {
            collect_opts(o, &mut result);
        }
        result
    }
}

fn collect_opts<'a>(o: &'a GeneratorOption, out: &mut Vec<&'a Opt>) {
    match o {
        GeneratorOption::Option(opt) => out.push(opt),
        GeneratorOption::Category(cat) => {
            for child in &cat.options {
                collect_opts(child, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_template_yaml() {
        let t = load().expect("template.yaml should parse");
        assert!(t.required.contains(&"chip".to_string()));
        let opts = t.flat_options();
        let names: Vec<&str> = opts.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"rtl8721f"), "expected rtl8721f option");
        assert!(names.contains(&"embassy"), "expected embassy option");
        assert!(names.contains(&"wifi"), "expected wifi option");
    }
}
