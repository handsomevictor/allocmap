/// IPC channel for sending allocation events from the injected .so
/// to the allocmap-cli process.
///
/// Communication is done via a Unix domain socket whose path is passed
/// through the ALLOCMAP_SOCKET_PATH environment variable.
///
/// # Safety
/// `send_event` must not allocate. It casts the event struct to raw bytes
/// and writes them directly. `std::env::var` allocates, but `init()` is
/// called only at library load time, before the hooks are active.
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;

/// Global IPC connection (lazily initialized at library load time)
static IPC: Mutex<Option<UnixStream>> = Mutex::new(None);

/// One-time initialization guard to prevent double-connect
static IPC_INIT: std::sync::Once = std::sync::Once::new();

/// Environment variable name for the socket path
pub const SOCKET_PATH_ENV: &str = "ALLOCMAP_SOCKET_PATH";

/// An allocation event transmitted from the target process to allocmap-cli.
///
/// `repr(C)` ensures a stable, predictable memory layout for the binary wire
/// format. The receiver reads exactly `size_of::<AllocEvent>()` bytes.
///
/// Note: there are 7 bytes of padding after `event_type` due to u64 alignment.
/// The receiver must use the same struct definition for correct deserialization.
#[repr(C)]
pub struct AllocEvent {
    /// 1 = alloc, 2 = free
    pub event_type: u8,
    /// Address returned by malloc / passed to free
    pub address: u64,
    /// Allocation size in bytes (0 for free events)
    pub size: u64,
    /// Monotonic timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Initialize the IPC connection.
/// Safe to call multiple times; subsequent calls are no-ops.
/// Returns `true` if the connection was established successfully.
pub fn init() -> bool {
    let mut connected = false;
    IPC_INIT.call_once(|| {
        let path = match std::env::var(SOCKET_PATH_ENV) {
            Ok(p) => p,
            Err(_) => return, // Not running under allocmap supervision
        };

        if let Ok(stream) = UnixStream::connect(&path) {
            // Non-blocking so send_event never stalls the target process.
            let _ = stream.set_nonblocking(true);
            let mut guard = match IPC.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            *guard = Some(stream);
            connected = true;
        }
    });
    connected
}

/// Send an `AllocEvent` through the IPC channel.
///
/// Uses `try_lock` so the target process is never blocked waiting for the
/// mutex. Events are silently dropped when the channel is busy or not
/// connected — reliability is sacrificed to protect process stability.
pub fn send_event(event: &AllocEvent) {
    // SAFETY: AllocEvent is repr(C) and contains only Copy types; no padding
    // bytes exist between fields on any supported platform.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (event as *const AllocEvent).cast::<u8>(),
            std::mem::size_of::<AllocEvent>(),
        )
    };

    if let Ok(mut guard) = IPC.try_lock() {
        if let Some(ref mut stream) = *guard {
            // Length-prefixed wire format: [4-byte LE length][AllocEvent bytes]
            let len = bytes.len() as u32;
            let _ = stream.write_all(&len.to_le_bytes());
            let _ = stream.write_all(bytes);
        }
    }
}

/// Send a raw byte payload through the IPC channel (used by tests / legacy).
/// Returns false if the channel is not connected or the send fails.
pub fn send(data: &[u8]) -> bool {
    let mut guard = match IPC.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };

    if let Some(ref mut stream) = *guard {
        // Write length-prefixed message: [4-byte LE length][data]
        let len = data.len() as u32;
        let len_bytes = len.to_le_bytes();
        if stream.write_all(&len_bytes).is_err() {
            return false;
        }
        if stream.write_all(data).is_err() {
            return false;
        }
        true
    } else {
        false
    }
}

/// Check if the IPC channel is connected
pub fn is_connected() -> bool {
    IPC.lock().map(|g| g.is_some()).unwrap_or(false)
}
