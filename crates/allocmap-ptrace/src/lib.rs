/// allocmap-ptrace: ptrace-based sampling implementation.
/// Linux uses PTRACE_ATTACH; macOS uses a sysctl/ps-based stub.
// ---- Linux modules ----
#[cfg(target_os = "linux")]
pub mod attach;
#[cfg(target_os = "linux")]
pub mod sampler;
#[cfg(target_os = "linux")]
pub mod backtrace;
#[cfg(target_os = "linux")]
pub mod symbols;
#[cfg(target_os = "linux")]
pub mod error;

#[cfg(target_os = "linux")]
pub use sampler::PtraceSampler;
#[cfg(target_os = "linux")]
pub use error::PtraceError;

// ---- macOS stub ----
#[cfg(target_os = "macos")]
pub mod macos_sampler;
#[cfg(target_os = "macos")]
pub use macos_sampler::PtraceSampler;
#[cfg(target_os = "macos")]
pub use macos_sampler::MacosSamplerError as PtraceError;

// ---- Unsupported platforms ----
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!(
    "allocmap-ptrace only supports Linux and macOS."
);
