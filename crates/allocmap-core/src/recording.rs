use serde::{Deserialize, Serialize};
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
