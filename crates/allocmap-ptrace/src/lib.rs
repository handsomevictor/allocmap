/// allocmap-ptrace：ptrace 采样实现
/// 仅支持 Linux，通过 cfg 宏隔离

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

/// 在非 Linux 平台上提供友好的编译错误
#[cfg(not(target_os = "linux"))]
compile_error!(
    "allocmap-ptrace only supports Linux. \
     On macOS, use allocmap-mach (Phase 2)."
);
