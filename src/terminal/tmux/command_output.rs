use std::{
    process::{Command, Output},
    time::Duration,
};

use anyhow::Result;

/// Execute a command while draining both pipes until EOF. `max_bytes` is the
/// aggregate limit for stdout plus stderr; output is never truncated as success.
///
/// Thin delegate kept for tmux call sites; the implementation lives in
/// [`crate::process_exec::bounded_output`].
pub(in crate::terminal) fn bounded_command_output(
    command: Command,
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    crate::process_exec::bounded_output(command, budget, max_bytes)
}
