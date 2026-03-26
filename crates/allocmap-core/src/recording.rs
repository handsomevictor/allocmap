use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::sample::SampleFrame;

/// .amr 文件的文件头
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingHeader {
    /// 文件魔数版本
    pub version: u32,
    /// 目标进程 PID
    pub pid: u32,
    /// 目标进程名
    pub program_name: String,
    /// 录制开始时间（Unix timestamp ms）
    pub start_time_ms: u64,
    /// 采样频率（Hz）
    pub sample_rate_hz: u32,
    /// 总帧数（用于读取时确定帧范围）
    pub frame_count: u64,
}

/// .amr 文件的文件尾
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFooter {
    /// 录制结束时间
    pub end_time_ms: u64,
    /// 总采样帧数
    pub total_frames: u64,
    /// 峰值堆内存
    pub peak_heap_bytes: u64,
    /// 平均堆内存
    pub avg_heap_bytes: u64,
}

/// 完整的 .amr 录制数据
#[derive(Debug, Clone)]
pub struct AllocMapRecording {
    pub header: RecordingHeader,
    pub frames: Vec<SampleFrame>,
    pub footer: RecordingFooter,
}

/// .amr 文件魔数
pub const AMR_MAGIC: &[u8; 4] = b"AMR\0";
/// 当前格式版本
pub const AMR_VERSION: u32 = 1;

impl AllocMapRecording {
    /// Write the recording to a writer in .amr binary format.
    ///
    /// Format layout:
    ///   magic(4) | version(4 LE) | hdr_len(4 LE) | hdr_json |
    ///   [frame_len(4 LE) | frame_bincode]* |
    ///   foot_len(4 LE) | foot_json
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), CoreError> {
        // 1. Magic bytes
        writer.write_all(AMR_MAGIC).map_err(CoreError::Io)?;

        // 2. Version (u32 little-endian)
        writer
            .write_all(&AMR_VERSION.to_le_bytes())
            .map_err(CoreError::Io)?;

        // 3. Header JSON with frame_count baked in
        let mut header = self.header.clone();
        header.frame_count = self.frames.len() as u64;
        let hdr_bytes =
            serde_json::to_vec(&header).map_err(|e| CoreError::Serialization(e.to_string()))?;
        let hdr_len = hdr_bytes.len() as u32;
        writer
            .write_all(&hdr_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&hdr_bytes).map_err(CoreError::Io)?;

        // 4. Frames (bincode)
        for frame in &self.frames {
            let frame_bytes = bincode::serialize(frame)
                .map_err(|e| CoreError::Serialization(e.to_string()))?;
            let frame_len = frame_bytes.len() as u32;
            writer
                .write_all(&frame_len.to_le_bytes())
                .map_err(CoreError::Io)?;
            writer.write_all(&frame_bytes).map_err(CoreError::Io)?;
        }

        // 5. Footer JSON
        let foot_bytes = serde_json::to_vec(&self.footer)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;
        let foot_len = foot_bytes.len() as u32;
        writer
            .write_all(&foot_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&foot_bytes).map_err(CoreError::Io)?;

        Ok(())
    }

    /// Read and deserialize an .amr recording from a reader.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, CoreError> {
        // 1. Read and verify magic bytes
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(CoreError::Io)?;
        if &magic != AMR_MAGIC {
            return Err(CoreError::InvalidRecording(
                "Invalid magic bytes; expected AMR\\0".to_string(),
            ));
        }

        // 2. Read and verify version
        let mut ver_buf = [0u8; 4];
        reader.read_exact(&mut ver_buf).map_err(CoreError::Io)?;
        let version = u32::from_le_bytes(ver_buf);
        if version != AMR_VERSION {
            return Err(CoreError::UnsupportedVersion {
                expected: AMR_VERSION,
                got: version,
            });
        }

        // 3. Read header JSON
        let mut hdr_len_buf = [0u8; 4];
        reader
            .read_exact(&mut hdr_len_buf)
            .map_err(CoreError::Io)?;
        let hdr_len = u32::from_le_bytes(hdr_len_buf) as usize;
        let mut hdr_bytes = vec![0u8; hdr_len];
        reader.read_exact(&mut hdr_bytes).map_err(CoreError::Io)?;
        let header: RecordingHeader = serde_json::from_slice(&hdr_bytes)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;

        let frame_count = header.frame_count as usize;

        // 4. Read exactly frame_count frames
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let mut frame_len_buf = [0u8; 4];
            reader
                .read_exact(&mut frame_len_buf)
                .map_err(CoreError::Io)?;
            let frame_len = u32::from_le_bytes(frame_len_buf) as usize;
            let mut frame_bytes = vec![0u8; frame_len];
            reader
                .read_exact(&mut frame_bytes)
                .map_err(CoreError::Io)?;
            let frame: SampleFrame = bincode::deserialize(&frame_bytes)
                .map_err(|e| CoreError::Serialization(e.to_string()))?;
            frames.push(frame);
        }

        // 5. Read footer JSON
        let mut foot_len_buf = [0u8; 4];
        reader
            .read_exact(&mut foot_len_buf)
            .map_err(CoreError::Io)?;
        let foot_len = u32::from_le_bytes(foot_len_buf) as usize;
        let mut foot_bytes = vec![0u8; foot_len];
        reader.read_exact(&mut foot_bytes).map_err(CoreError::Io)?;
        let footer: RecordingFooter = serde_json::from_slice(&foot_bytes)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;

        Ok(AllocMapRecording {
            header,
            frames,
            footer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{AllocationSite, StackFrame};
    use std::io::Cursor;

    fn make_recording() -> AllocMapRecording {
        let header = RecordingHeader {
            version: AMR_VERSION,
            pid: 42,
            program_name: "test_prog".to_string(),
            start_time_ms: 1_000_000,
            sample_rate_hz: 10,
            frame_count: 0, // will be set by write_to
        };

        let frame1 = SampleFrame {
            timestamp_ms: 0,
            live_heap_bytes: 1024,
            alloc_rate: 512.0,
            free_rate: 128.0,
            top_sites: vec![AllocationSite {
                live_bytes: 512,
                alloc_count: 4,
                peak_bytes: 512,
                frames: vec![StackFrame {
                    ip: 0xdeadbeef,
                    function: Some("allocate_stuff".to_string()),
                    file: Some("main.rs".to_string()),
                    line: Some(42),
                }],
            }],
            thread_count: 1,
            thread_ids: vec![],
        };

        let frame2 = SampleFrame {
            timestamp_ms: 100,
            live_heap_bytes: 2048,
            alloc_rate: 1024.0,
            free_rate: 256.0,
            top_sites: vec![],
            thread_count: 1,
            thread_ids: vec![],
        };

        let footer = RecordingFooter {
            end_time_ms: 1_000_100,
            total_frames: 2,
            peak_heap_bytes: 2048,
            avg_heap_bytes: 1536,
        };

        AllocMapRecording {
            header,
            frames: vec![frame1, frame2],
            footer,
        }
    }

    #[test]
    fn test_roundtrip_write_read() {
        let original = make_recording();

        // Write to in-memory buffer
        let mut buf = Vec::new();
        original.write_to(&mut buf).expect("write_to should succeed");

        // Read back from buffer
        let mut cursor = Cursor::new(&buf);
        let recovered =
            AllocMapRecording::read_from(&mut cursor).expect("read_from should succeed");

        // Verify header fields (frame_count gets set during write)
        assert_eq!(recovered.header.pid, original.header.pid);
        assert_eq!(
            recovered.header.program_name,
            original.header.program_name
        );
        assert_eq!(
            recovered.header.start_time_ms,
            original.header.start_time_ms
        );
        assert_eq!(
            recovered.header.sample_rate_hz,
            original.header.sample_rate_hz
        );
        assert_eq!(recovered.header.frame_count, 2);

        // Verify frames
        assert_eq!(recovered.frames.len(), 2);
        assert_eq!(
            recovered.frames[0].live_heap_bytes,
            original.frames[0].live_heap_bytes
        );
        assert_eq!(
            recovered.frames[0].timestamp_ms,
            original.frames[0].timestamp_ms
        );
        assert_eq!(
            recovered.frames[0].top_sites.len(),
            original.frames[0].top_sites.len()
        );
        assert_eq!(
            recovered.frames[0].top_sites[0].live_bytes,
            original.frames[0].top_sites[0].live_bytes
        );
        assert_eq!(
            recovered.frames[0].top_sites[0].frames[0].function,
            original.frames[0].top_sites[0].frames[0].function
        );
        assert_eq!(
            recovered.frames[1].live_heap_bytes,
            original.frames[1].live_heap_bytes
        );

        // Verify footer
        assert_eq!(
            recovered.footer.peak_heap_bytes,
            original.footer.peak_heap_bytes
        );
        assert_eq!(
            recovered.footer.total_frames,
            original.footer.total_frames
        );
        assert_eq!(
            recovered.footer.avg_heap_bytes,
            original.footer.avg_heap_bytes
        );
    }

    #[test]
    fn test_invalid_magic_returns_error() {
        // Build a buffer with wrong magic
        let mut buf = Vec::new();
        buf.extend_from_slice(b"BAD!"); // wrong magic
        buf.extend_from_slice(&AMR_VERSION.to_le_bytes());

        let mut cursor = Cursor::new(&buf);
        let result = AllocMapRecording::read_from(&mut cursor);

        match result {
            Err(CoreError::InvalidRecording(msg)) => {
                assert!(
                    msg.contains("magic") || msg.contains("AMR"),
                    "Error message should mention magic: {msg}"
                );
            }
            other => panic!("Expected InvalidRecording, got: {other:?}"),
        }
    }

    #[test]
    fn test_version_mismatch_returns_error() {
        // Build a buffer with correct magic but wrong version
        let mut buf = Vec::new();
        buf.extend_from_slice(AMR_MAGIC);
        let wrong_version: u32 = 99;
        buf.extend_from_slice(&wrong_version.to_le_bytes());

        let mut cursor = Cursor::new(&buf);
        let result = AllocMapRecording::read_from(&mut cursor);

        match result {
            Err(CoreError::UnsupportedVersion { expected, got }) => {
                assert_eq!(expected, AMR_VERSION);
                assert_eq!(got, 99);
            }
            other => panic!("Expected UnsupportedVersion, got: {other:?}"),
        }
    }
}
