use std::fmt;
use std::sync::OnceLock;

/// Log severity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Verbose tracing output for fine-grained emulation details.
    Trace,
    /// Informational messages about normal emulator operation.
    Info,
    /// Warnings about unexpected or potentially incorrect conditions.
    Warn,
}

/// Sink that receives log messages emitted by the emulator core.
///
/// # Examples
///
/// ```
/// use std::fmt;
/// use vibe_emu_core::diagnostics::{Level, LogSink, try_set_log_sink};
///
/// struct PrintSink;
///
/// impl LogSink for PrintSink {
///     fn log(&self, level: Level, target: &'static str, args: fmt::Arguments) {
///         eprintln!("[{target}] {level:?}: {args}");
///     }
/// }
/// ```
pub trait LogSink: Send + Sync + 'static {
    /// Receive a single log message at the given severity `level` from `target`.
    fn log(&self, level: Level, target: &'static str, args: fmt::Arguments);
}

static LOG_SINK: OnceLock<Box<dyn LogSink>> = OnceLock::new();

/// Attempt to install a global log sink.
///
/// Returns `Err(sink)` if a sink has already been installed.
///
/// # Examples
///
/// ```
/// use std::fmt;
/// use vibe_emu_core::diagnostics::{Level, LogSink, try_set_log_sink};
///
/// struct PrintSink;
///
/// impl LogSink for PrintSink {
///     fn log(&self, _level: Level, target: &'static str, args: fmt::Arguments) {
///         eprintln!("[{target}] {args}");
///     }
/// }
///
/// // Install once at startup. A second call would return Err.
/// let _ = try_set_log_sink(Box::new(PrintSink));
/// ```
pub fn try_set_log_sink(sink: Box<dyn LogSink>) -> Result<(), Box<dyn LogSink>> {
    LOG_SINK.set(sink)
}

/// Returns `true` if a global log sink has been installed.
pub fn has_log_sink() -> bool {
    LOG_SINK.get().is_some()
}

pub(crate) fn emit(level: Level, target: &'static str, args: fmt::Arguments) {
    if let Some(sink) = LOG_SINK.get() {
        sink.log(level, target, args);
    }
}
