use anyhow::{Result, anyhow, bail};

/// Process a single template file's content.
///
/// # Directives
///
/// All directives are recognized anywhere a line begins with optional
/// whitespace followed by `#` or `//`.
///
/// ## REPLACE
/// ```text
/// #REPLACE {{placeholder}} varname
/// #REPLACE {{a}} var_a && {{b}} var_b
/// ```
/// Declares that `{{placeholder}}` should be replaced with the value of
/// `varname` in all subsequent lines.  The directive line itself is not
/// emitted.  Unknown `varname` silently leaves the placeholder as-is.
///
/// ## IF / ELIF / ELSE / ENDIF
/// ```text
/// //IF option("embassy")
/// ... lines emitted when "embassy" is in selected options ...
/// //ELIF option("wifi")
/// ... lines emitted when "wifi" is selected (and "embassy" is not) ...
/// //ELSE
/// ... lines emitted otherwise ...
/// //ENDIF
/// ```
/// Condition: `option("name")` is true when `name` is present in the
/// `selected` slice.  Blocks may nest arbitrarily.  Directive lines are
/// not emitted.
///
/// ## INCLUDEFILE
/// When the *first* line of a file is `//INCLUDEFILE option("name")` and
/// `name` is **not** in `selected`, `process_file` returns `Ok(None)` and the
/// file is not written.
///
/// # Errors
///
/// Malformed directives are hard errors, never silently ignored — a code
/// generator that quietly drops a block produces a broken project whose
/// compile error shows up far from the actual mistake.  Rejected:
/// an `//IF` that is never closed, an `//ELIF` / `//ELSE` / `//ENDIF` with no
/// matching `//IF`, an `//IF` / `//ELIF` with no condition, and any condition
/// that is not exactly `option("name")`.
///
/// # Returns
/// `Ok(None)` – file should be skipped.
/// `Ok(Some(text))` – processed text to write.
pub fn process_file(
    content: &str,
    vars: &[(&str, &str)],
    selected: &[&str],
) -> Result<Option<String>> {
    // Start with a copy of the caller-supplied variable bindings.
    let mut resolved: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let mut out = String::with_capacity(content.len());

    // Stack of conditional blocks.
    // Each entry is (output_active, already_output, opened_at):
    //   output_active  – current branch should emit lines
    //   already_output – a prior branch of this IF/ELIF chain already emitted
    //   opened_at      – 1-based line of the `//IF`, for unclosed-block errors
    let mut block_stack: Vec<(bool, bool, usize)> = Vec::new();

    let mut lines = content.lines().enumerate().peekable();

    // --- INCLUDEFILE: check first line only ---
    if let Some(&(_, first)) = lines.peek()
        && let Some(cond) = extract_directive(first.trim(), "INCLUDEFILE ")
    {
        lines.next(); // consume the directive line
        if !eval_condition(cond.trim(), selected).map_err(|e| anyhow!("line 1: {}", e))? {
            return Ok(None);
        }
    }

    for (idx, line) in lines {
        let lineno = idx + 1;
        let trimmed = line.trim();

        // --- Block control directives ---
        if let Some(cond) = extract_directive(trimmed, "IF ") {
            let active = eval_condition(cond.trim(), selected)
                .map_err(|e| anyhow!("line {}: {}", lineno, e))?;
            block_stack.push((active, active, lineno));
            continue;
        }
        if trimmed == "//IF" || trimmed == "#IF" {
            bail!(
                "line {}: //IF requires a condition, e.g. //IF option(\"embassy\")",
                lineno
            );
        }
        if let Some(cond) = extract_directive(trimmed, "ELIF ") {
            let (already, opened_at) = match block_stack.last() {
                Some(&(_, already, opened_at)) => (already, opened_at),
                None => bail!("line {}: stray //ELIF with no matching //IF", lineno),
            };
            let active = !already
                && eval_condition(cond.trim(), selected)
                    .map_err(|e| anyhow!("line {}: {}", lineno, e))?;
            *block_stack.last_mut().expect("checked non-empty above") =
                (active, already || active, opened_at);
            continue;
        }
        if trimmed == "//ELIF" || trimmed == "#ELIF" {
            bail!(
                "line {}: //ELIF requires a condition, e.g. //ELIF option(\"wifi\")",
                lineno
            );
        }
        if trimmed == "//ELSE" || trimmed == "#ELSE" {
            match block_stack.last_mut() {
                Some(top) => {
                    let already = top.1;
                    *top = (!already, true, top.2);
                }
                None => bail!("line {}: stray //ELSE with no matching //IF", lineno),
            }
            continue;
        }
        if trimmed == "//ENDIF" || trimmed == "#ENDIF" {
            if block_stack.pop().is_none() {
                bail!("line {}: stray //ENDIF with no matching //IF", lineno);
            }
            continue;
        }

        // --- REPLACE directive ---
        if let Some(rest) = extract_directive(trimmed, "REPLACE ") {
            // Only process if all enclosing blocks are active.
            if block_stack.iter().all(|&(active, _, _)| active) {
                parse_replace_directive(rest, vars, &mut resolved);
            }
            continue;
        }

        // --- Emit line if every enclosing block is active ---
        let emit = block_stack.iter().all(|&(active, _, _)| active);
        if emit {
            out.push_str(&substitute_owned(line, &resolved));
            out.push('\n');
        }
    }

    if let Some(&(_, _, opened_at)) = block_stack.last() {
        bail!("unclosed //IF opened at line {}", opened_at);
    }

    Ok(Some(out))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try to strip `#<directive>` or `//<directive>` from `line`.
/// Returns the text after the directive prefix, or `None`.
fn extract_directive<'a>(line: &'a str, directive: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("#{}", directive))
        .or_else(|| line.strip_prefix(&format!("//{}", directive)))
}

/// Evaluate a condition expression.
///
/// `option("name")` is the only accepted form.  Anything else is an error
/// rather than a `false` — a typo like `option("embassy"` must not silently
/// delete the block it guards.
fn eval_condition(expr: &str, selected: &[&str]) -> Result<bool> {
    let inner = expr
        .strip_prefix("option(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .ok_or_else(|| {
            anyhow!(
                "malformed condition {:?}: the only supported form is option(\"name\")",
                expr
            )
        })?;
    Ok(selected.contains(&inner))
}

/// Parse a REPLACE directive body and append new bindings to `resolved`.
fn parse_replace_directive(
    body: &str,
    vars: &[(&str, &str)],
    resolved: &mut Vec<(String, String)>,
) {
    for pair in body.split("&&") {
        let pair = pair.trim();
        let mut tokens = pair.splitn(2, ' ');
        let placeholder = match tokens.next() {
            Some(p) => p.trim().to_string(),
            None => continue,
        };
        let varname = match tokens.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        if let Some(&(_, value)) = vars.iter().find(|(k, _)| *k == varname) {
            resolved.push((
                placeholder
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .to_string(),
                value.to_string(),
            ));
        }
    }
}

/// Replace every `{{key}}` occurrence in `s` with the corresponding value.
#[allow(dead_code)] // used in tests and will be called from tui.rs in Step 11
pub fn substitute(s: &str, vars: &[(&str, &str)]) -> String {
    let mut out = s.to_owned();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        out = out.replace(&placeholder, value);
    }
    out
}

fn substitute_owned(s: &str, vars: &[(String, String)]) -> String {
    let mut out = s.to_owned();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        out = out.replace(&placeholder, value);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pf<'a>(content: &'a str, vars: &[(&'a str, &'a str)], selected: &[&'a str]) -> String {
        process_file(content, vars, selected).unwrap().unwrap()
    }

    // --- REPLACE ---

    #[test]
    fn replace_single_pair() {
        let c = "#REPLACE {{name}} project-name\nname = \"{{name}}\"\n";
        assert_eq!(
            pf(c, &[("project-name", "my-app")], &[]),
            "name = \"my-app\"\n"
        );
    }

    #[test]
    fn replace_multi_pair() {
        let c = "#REPLACE {{a}} key-a && {{b}} key-b\n{{a}} and {{b}}\n";
        assert_eq!(
            pf(c, &[("key-a", "hello"), ("key-b", "world")], &[]),
            "hello and world\n"
        );
    }

    #[test]
    fn replace_unknown_varname_ignored() {
        let c = "#REPLACE {{x}} no-such-var\n{{x}} stays\n";
        assert_eq!(pf(c, &[], &[]), "{{x}} stays\n");
    }

    #[test]
    fn plain_substitution() {
        let c = "name = \"{{project-name}}\"\n";
        assert_eq!(pf(c, &[("project-name", "foo")], &[]), "name = \"foo\"\n");
    }

    // --- IF / ELSE / ENDIF ---

    #[test]
    fn if_true_branch_emitted() {
        let c = "//IF option(\"embassy\")\nembassy line\n//ENDIF\n";
        assert_eq!(pf(c, &[], &["embassy"]), "embassy line\n");
    }

    #[test]
    fn if_false_branch_suppressed() {
        let c = "//IF option(\"embassy\")\nembassy line\n//ENDIF\n";
        assert_eq!(pf(c, &[], &[]), "");
    }

    #[test]
    fn if_else() {
        let c = "//IF option(\"embassy\")\nasync\n//ELSE\nbare\n//ENDIF\n";
        assert_eq!(pf(c, &[], &["embassy"]), "async\n");
        assert_eq!(pf(c, &[], &[]), "bare\n");
    }

    #[test]
    fn if_elif_else() {
        let c = "//IF option(\"a\")\nA\n//ELIF option(\"b\")\nB\n//ELSE\nC\n//ENDIF\n";
        assert_eq!(pf(c, &[], &["a"]), "A\n");
        assert_eq!(pf(c, &[], &["b"]), "B\n");
        assert_eq!(pf(c, &[], &[]), "C\n");
        // Both selected: first branch wins
        assert_eq!(pf(c, &[], &["a", "b"]), "A\n");
    }

    #[test]
    fn nested_if() {
        let c = "//IF option(\"a\")\n//IF option(\"b\")\nAB\n//ENDIF\n//ENDIF\n";
        assert_eq!(pf(c, &[], &["a", "b"]), "AB\n");
        assert_eq!(pf(c, &[], &["a"]), "");
        assert_eq!(pf(c, &[], &[]), "");
    }

    // --- INCLUDEFILE ---

    #[test]
    fn includefile_present() {
        let c = "//INCLUDEFILE option(\"wifi\")\ncontent\n";
        assert!(process_file(c, &[], &["wifi"]).unwrap().is_some());
    }

    #[test]
    fn includefile_absent() {
        let c = "//INCLUDEFILE option(\"wifi\")\ncontent\n";
        assert!(process_file(c, &[], &[]).unwrap().is_none());
    }

    // --- Syntax errors must not be silent ---

    #[test]
    fn stray_endif_is_error() {
        let c = "line\n//ENDIF\n";
        let e = process_file(c, &[], &[]).unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
        assert!(e.contains("stray //ENDIF"), "{e}");
    }

    #[test]
    fn stray_else_is_error() {
        let c = "line\n//ELSE\nother\n";
        assert!(process_file(c, &[], &[]).is_err());
    }

    #[test]
    fn unclosed_if_is_error() {
        let c = "//IF option(\"a\")\nA\n";
        let e = process_file(c, &[], &["a"]).unwrap_err().to_string();
        assert!(e.contains("unclosed //IF opened at line 1"), "{e}");
    }

    #[test]
    fn extra_endif_is_error() {
        // The second //ENDIF pops an empty stack; without this check the
        // "secret" line would leak into the output.
        let c = "//IF option(\"a\")\nA\n//ENDIF\n//ENDIF\nsecret\n";
        assert!(process_file(c, &[], &[]).is_err());
    }

    #[test]
    fn malformed_condition_is_error() {
        // Missing closing paren — previously fell back to a bare-name compare
        // and evaluated to false, silently deleting the guarded block.
        let c = "//IF option(\"embassy\"\nA\n//ENDIF\n";
        let e = process_file(c, &[], &["embassy"]).unwrap_err().to_string();
        assert!(e.contains("line 1: malformed condition"), "{e}");
    }

    #[test]
    fn bare_option_name_is_error() {
        let c = "//IF embassy\nA\n//ENDIF\n";
        assert!(process_file(c, &[], &["embassy"]).is_err());
    }

    #[test]
    fn if_without_condition_is_error() {
        let c = "//IF\nA\n//ENDIF\n";
        assert!(process_file(c, &[], &[]).is_err());
    }

    #[test]
    fn malformed_condition_inside_inactive_block_is_error() {
        // Validation is static: a typo in a branch that happens not to be
        // taken is still a template bug.
        let c = "//IF option(\"a\")\n//IF option(\"b\"\nX\n//ENDIF\n//ENDIF\n";
        assert!(process_file(c, &[], &[]).is_err());
    }
}
