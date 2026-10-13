//! Environment variable retrieval.

/// Retrieves environment variables from the host (what the program was started with).
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars()
}
