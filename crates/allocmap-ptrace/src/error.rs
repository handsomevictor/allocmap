use thiserror::Error;

#[derive(Debug, Error)]
pub enum PtraceError {
    #[error(
        "Failed to attach to PID {pid}: process not found. \
         Make sure the process is running: ps aux | grep {pid}"
    )]
    ProcessNotFound { pid: u32 },

    #[error(
        "Failed to attach to PID {pid}: permission denied (os error {errno}). \
         Try: sudo allocmap attach --pid {pid}\n\
         Or allow ptrace: echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope"
    )]
    PermissionDenied { pid: u32, errno: i32 },

    #[error(
        "Failed to attach to PID {pid}: process is already being traced by another debugger."
    )]
    AlreadyTraced { pid: u32 },

    #[error("ptrace operation failed on PID {pid}: {detail}")]
    PtraceOp { pid: u32, detail: String },

    #[error("Failed to read /proc/{pid}/maps: {detail}")]
    ProcMaps { pid: u32, detail: String },

    #[error("Symbol resolution failed for address 0x{addr:016x}: {detail}")]
    SymbolResolution { addr: u64, detail: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
