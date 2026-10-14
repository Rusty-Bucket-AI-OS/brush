//! Environment variable retrieval.

/// The variable a background job is started with: the `$$` of the script that started it. It is
/// the shell's own and is not shown to the script as a variable.
pub(crate) const PARENT_PID_VAR: &str = "RB_BRUSH_SHELL_PID";

/// Retrieves environment variables from the host (what the program was started with).
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars().filter(|(name, _)| name != PARENT_PID_VAR)
}
