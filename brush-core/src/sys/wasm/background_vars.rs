//! The variables a background job is started with. The job is a new shell, so the variables its
//! command uses travel as `declare` lines that carry every attribute (read-only, integer, case
//! conversion, name reference, array kind, export). A variable that cannot be carried makes the
//! job refuse to start: a job never runs with fewer protections than the script that started it.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::variables::{ShellValue, ShellVariable};
use crate::{Shell, error, extensions};

/// Variables every shell of this kind already has, read-only, with the right value in the job.
const SHELL_OWN: [&str; 7] = [
    "BASHOPTS",
    "BASH_VERSINFO",
    "EUID",
    "PPID",
    "SHELLOPTS",
    "UID",
    "_",
];

/// The words (runs of letters, digits and underscores) of a text.
pub(super) fn words(text: &str) -> HashSet<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn refuse(name: &str, why: &str) -> error::Error {
    error::ErrorKind::FailedToExecuteCommand(
        "background job".to_owned(),
        std::io::Error::other(format!(
            "the job cannot carry the variable {name} ({why}); it was not started"
        )),
    )
    .into()
}

/// The names the job needs: the words of `scope`, and the targets of name references among them.
fn needed_names<SE: extensions::ShellExtensions>(
    shell: &Shell<SE>,
    scope: &str,
) -> HashSet<String> {
    let mut needed: HashSet<String> = words(scope).into_iter().map(str::to_owned).collect();
    loop {
        let before = needed.len();
        for (name, var) in shell.env().iter() {
            if var.is_treated_as_nameref()
                && needed.contains(name.as_str())
                && let ShellValue::String(target) = var.value()
            {
                needed.extend(words(target).into_iter().map(str::to_owned));
            }
        }
        if needed.len() == before {
            return needed;
        }
    }
}

/// The `declare` line (or plain assignment) for one variable, or `None` when the job needs no
/// line for it.
fn line_for<SE: extensions::ShellExtensions>(
    shell: &Shell<SE>,
    name: &str,
    var: &ShellVariable,
) -> Result<Option<String>, error::Error> {
    let value = var.value();
    if matches!(value, ShellValue::Dynamic { .. }) {
        return Ok(None);
    }
    let flags = var.attribute_flags(shell);
    // An exported variable with no other attribute travels in the environment.
    if flags == "x" {
        return Ok(None);
    }
    let keyword = if flags.is_empty() {
        String::new()
    } else {
        format!("declare -{flags} ")
    };
    match value {
        ShellValue::Unset(_) if flags.is_empty() => Ok(None),
        ShellValue::Unset(_) => Ok(Some(format!("{keyword}{name}"))),
        _ => {
            let assignable = value
                .to_assignable_str(None, shell)
                .map_err(|_| refuse(name, "its value cannot be written out"))?;
            Ok(Some(format!("{keyword}{name}={assignable}")))
        }
    }
}

/// The lines that give a job the variables `scope` (the job's text and the functions it uses)
/// mentions, and every read-only variable, with their attributes. Name references come last.
pub(super) fn declarations<SE: extensions::ShellExtensions>(
    shell: &Shell<SE>,
    scope: &str,
) -> Result<String, error::Error> {
    let needed = needed_names(shell, scope);
    let mut vars: Vec<_> = shell
        .env()
        .iter()
        .filter(|(name, var)| {
            !SHELL_OWN.contains(&name.as_str())
                && (var.is_readonly() || needed.contains(name.as_str()))
        })
        .collect();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    let (refs, plain): (Vec<_>, Vec<_>) = vars
        .into_iter()
        .partition(|(_, var)| var.is_treated_as_nameref());
    let mut out = String::new();
    for (name, var) in plain.into_iter().chain(refs) {
        if !crate::env::valid_variable_name(name) {
            if var.is_readonly() {
                return Err(refuse(name, "its name is not a valid variable name"));
            }
            continue;
        }
        if let Some(line) = line_for(shell, name, var)? {
            let _ = writeln!(out, "{line}");
        }
    }
    Ok(out)
}
