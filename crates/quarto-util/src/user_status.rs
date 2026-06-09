//! User-facing status output for Quarto CLI binaries.
//!
//! The workspace uses `tracing` for telemetry, and the default
//! `EnvFilter` directive (see [`crate::verbose_to_filter`]) is
//! `quarto=warn`. That means `tracing::info!` calls are suppressed
//! unless the user passes `-v` — appropriate for developer telemetry,
//! but the wrong channel for messages the user is supposed to see
//! during a normal run (progress banners, "Rendered N of M files
//! to ...", post-render summaries).
//!
//! The [`user_status!`] macro is the explicit user-facing channel.
//! It writes to stderr, is silenced by `--quiet`, and bypasses
//! `EnvFilter` entirely. The convention is:
//!
//! - `tracing::info!`/`debug!`/`trace!` — developer telemetry,
//!   opt-in via `-v` or `RUST_LOG`.
//! - `user_status!(quiet, …)` — messages the end user should see
//!   during normal operation.
//!
//! `quiet` is threaded through explicitly at the call site (rather
//! than read from process-global state) so callers can construct
//! and test it without setting up a logger.

/// Emit a status line intended for the end user.
///
/// Writes to stderr unless `$quiet` is true. Formatting mirrors
/// `eprintln!` — supply a format string and arguments.
///
/// ```ignore
/// use quarto_util::user_status;
/// let quiet = false;
/// user_status!(quiet, "Rendered {} of {} files to {}", 5, 5, "_site");
/// ```
#[macro_export]
macro_rules! user_status {
    ($quiet:expr, $($arg:tt)*) => {{
        if !$quiet {
            eprintln!($($arg)*);
        }
    }};
}
