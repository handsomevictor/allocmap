use anyhow::Result;
use std::collections::HashMap;
use allocmap_core::StackFrame;

/// Symbol resolver: maps instruction pointer addresses to function names
/// by reading the target process's ELF debug info
pub struct SymbolResolver {
    /// Cache: address -> resolved StackFrame
    cache: HashMap<u64, StackFrame>,
}

impl SymbolResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Resolve an address to a StackFrame.
    /// Uses a cache to avoid repeated lookups.
    pub fn resolve(&mut self, ip: u64, pid: u32) -> StackFrame {
        if let Some(cached) = self.cache.get(&ip) {
            return cached.clone();
        }

        let frame = self.resolve_uncached(ip, pid);
        self.cache.insert(ip, frame.clone());
        frame
    }

    fn resolve_uncached(&self, ip: u64, pid: u32) -> StackFrame {
        if let Ok(symbol) = self.lookup_symbol_in_proc(ip, pid) {
            return symbol;
        }
        // Try to at least identify the binary
        if let Ok(binary_name) = self.binary_name_for_ip(ip, pid) {
            return StackFrame {
                ip,
                function: Some(format!("<{}>", binary_name)),
                file: None,
                line: None,
            };
        }
        StackFrame { ip, function: None, file: None, line: None }
    }

    fn binary_name_for_ip(&self, ip: u64, pid: u32) -> Result<String> {
        let maps_path = format!("/proc/{}/maps", pid);
        let maps_content = std::fs::read_to_string(&maps_path)?;
        for line in maps_content.lines() {
            let parts: Vec<&str> = line.splitn(6, ' ').collect();
            if parts.len() < 6 { continue; }
            let path = parts[5].trim();
            if path.is_empty() { continue; }
            let range = parts[0];
            let addrs: Vec<&str> = range.splitn(2, '-').collect();
            if addrs.len() != 2 { continue; }
            let start = u64::from_str_radix(addrs[0], 16).unwrap_or(0);
            let end   = u64::from_str_radix(addrs[1], 16).unwrap_or(0);
            if ip >= start && ip < end {
                // Strip " (deleted)" suffix — binary was replaced on disk but
                // the kernel mapping still exists.
                let clean = path.trim_end_matches(" (deleted)");
                return Ok(std::path::Path::new(clean)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(clean)
                    .to_string());
            }
        }
        anyhow::bail!("IP not found in maps")
    }

    fn lookup_symbol_in_proc(&self, ip: u64, pid: u32) -> Result<StackFrame> {
        let maps_path = format!("/proc/{}/maps", pid);
        let maps_content = std::fs::read_to_string(&maps_path)?;

        struct MapEntry {
            start: u64,
            end: u64,
            file_offset: u64,
            /// Path with any " (deleted)" suffix stripped, used for ELF reads and entry matching.
            clean_path: String,
        }

        // Parse all relevant map entries (backed by a real file path)
        let mut entries: Vec<MapEntry> = Vec::new();
        for line in maps_content.lines() {
            let parts: Vec<&str> = line.splitn(6, ' ').collect();
            if parts.len() < 6 {
                continue;
            }
            let path = parts[5].trim();
            if path.is_empty() || !path.starts_with('/') {
                continue;
            }
            let range = parts[0];
            let addrs: Vec<&str> = range.splitn(2, '-').collect();
            if addrs.len() != 2 {
                continue;
            }
            let start = u64::from_str_radix(addrs[0], 16).unwrap_or(0);
            let end   = u64::from_str_radix(addrs[1], 16).unwrap_or(0);
            let file_offset = u64::from_str_radix(parts[2].trim(), 16).unwrap_or(0);
            // Strip " (deleted)" so addr2line can read the binary by its real path
            let clean_path = path.trim_end_matches(" (deleted)").to_string();
            entries.push(MapEntry { start, end, file_offset, clean_path });
        }

        // Find which entry contains `ip`
        let Some(hit) = entries.iter().find(|e| ip >= e.start && ip < e.end) else {
            anyhow::bail!("Address 0x{:016x} not found in /proc/{}/maps", ip, pid);
        };

        // For PIE executables and shared libraries the kernel places the binary
        // at a runtime `load_base`.  All ELF virtual addresses are relative to
        // that base.  The entry whose file_offset == 0 is always the first
        // PT_LOAD segment and tells us the base:
        //
        //   load_base = mmap_start  (where file_offset == 0 for this path)
        //   elf_vaddr = ip - load_base
        //
        // This is the address addr2line / DWARF expects.
        let load_base = entries.iter()
            .filter(|e| e.clean_path == hit.clean_path && e.file_offset == 0)
            .map(|e| e.start)
            .next()
            .unwrap_or(hit.start);

        let elf_vaddr = ip.saturating_sub(load_base);

        // If the on-disk file was replaced (cargo build), the clean_path may no longer
        // exist.  Fall back to /proc/PID/exe which still holds the original binary's fd.
        let readable = if std::path::Path::new(&hit.clean_path).exists() {
            hit.clean_path.clone()
        } else {
            std::fs::read_link(format!("/proc/{}/exe", pid))
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| hit.clean_path.clone())
        };

        self.resolve_with_addr2line(elf_vaddr, &readable, ip)
    }

    fn resolve_with_addr2line(&self, relative_ip: u64, binary_path: &str, raw_ip: u64) -> Result<StackFrame> {
        let debug = std::env::var("ALLOCMAP_DEBUG_SYMBOLS").is_ok();
        if debug {
            eprintln!("[debug sym] 0x{:x} -> {} (rel=0x{:x})", raw_ip, binary_path, relative_ip);
        }

        let data = std::fs::read(binary_path)?;

        // addr2line 0.22 uses object 0.35 internally; we use addr2line's re-exported object
        let file = addr2line::object::File::parse(&*data)?;

        // Check if there are any debug sections
        let has_debug = {
            use addr2line::object::Object;
            file.section_by_name(".debug_info").is_some()
        };

        if !has_debug {
            // No debug info: return just the binary name as context
            let binary_name = std::path::Path::new(binary_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(binary_path);

            if debug {
                eprintln!("[debug sym] 0x{:x}: no debug info in {}, using binary name", raw_ip, binary_name);
            }

            return Ok(StackFrame {
                ip: raw_ip,
                function: Some(format!("<{}>", binary_name)),
                file: None,
                line: None,
            });
        }

        // Use addr2line for full resolution
        let ctx = addr2line::Context::new(&file)?;

        // find_frames returns a LookupResult, not a Result — call `skip_all_loads()` to get frames synchronously
        let mut frames = ctx.find_frames(relative_ip).skip_all_loads()?;
        if let Some(frame) = frames.next()? {
            let function = frame.function.as_ref()
                .and_then(|f: &addr2line::FunctionName<_>| f.demangle().ok())
                .map(|s| s.to_string());
            let (file, line) = frame.location
                .map(|loc| (loc.file.map(|f: &str| f.to_string()), loc.line))
                .unwrap_or((None, None));

            if debug {
                eprintln!("[debug sym] 0x{:x}: resolved to {:?} at {:?}:{:?}", raw_ip, function, file, line);
            }

            return Ok(StackFrame {
                ip: raw_ip,
                function,
                file,
                line,
            });
        }

        Ok(StackFrame {
            ip: raw_ip,
            function: None,
            file: None,
            line: None,
        })
    }
}

impl Default for SymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_resolver_new_current_process() {
        // SymbolResolver::new() takes no args but resolve() reads /proc/<pid>/maps lazily.
        // Just verify construction succeeds.
        let resolver = SymbolResolver::new();
        assert!(
            resolver.cache.is_empty(),
            "New SymbolResolver should start with an empty cache"
        );
    }

    #[test]
    fn test_resolve_caches_result() {
        let mut resolver = SymbolResolver::new();
        let my_pid = std::process::id();
        // Address 0x1 is very unlikely to be a valid mapping,
        // so it falls back to a raw-address StackFrame.
        let frame1 = resolver.resolve(0x1, my_pid);
        let frame2 = resolver.resolve(0x1, my_pid);
        // Both calls should return the same ip.
        assert_eq!(frame1.ip, frame2.ip);
        // Cache should have exactly one entry.
        assert_eq!(resolver.cache.len(), 1);
    }

    #[test]
    fn test_resolve_unknown_address_returns_fallback() {
        let mut resolver = SymbolResolver::new();
        let my_pid = std::process::id();
        // Use an almost certainly invalid address.
        let frame = resolver.resolve(0x1, my_pid);
        assert_eq!(frame.ip, 0x1);
        assert!(
            frame.function.is_none(),
            "Unknown address should produce a None function name"
        );
    }
}
