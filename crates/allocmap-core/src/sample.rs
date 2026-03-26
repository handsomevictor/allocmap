use serde::{Deserialize, Serialize};

/// 一次采样帧：某个时间点的完整内存快照
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SampleFrame {
    /// 距录制开始的毫秒数
    pub timestamp_ms: u64,
    /// 当前时刻 live heap 字节数（已分配未释放）
    pub live_heap_bytes: u64,
    /// 分配速率（bytes/sec）
    pub alloc_rate: f64,
    /// 释放速率（bytes/sec）
    pub free_rate: f64,
    /// 本次采样的 top allocation sites
    pub top_sites: Vec<AllocationSite>,
    /// Number of threads active at sample time (Linux: from /proc/PID/task/)
    #[serde(default = "default_thread_count")]
    pub thread_count: u32,
    /// Thread IDs active at sample time (Linux: from /proc/PID/task/)
    #[serde(default)]
    pub thread_ids: Vec<u32>,
}

fn default_thread_count() -> u32 {
    1
}

/// 一个内存分配热点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationSite {
    /// 该 site 当前持有的 live bytes
    pub live_bytes: u64,
    /// 累计分配次数
    pub alloc_count: u64,
    /// 调用栈（从最内层到最外层）
    pub frames: Vec<StackFrame>,
}

/// 调用栈中的一帧
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// 原始指令指针地址
    pub ip: u64,
    /// 解析后的函数名（如果有调试符号）
    pub function: Option<String>,
    /// 源文件路径
    pub file: Option<String>,
    /// 源文件行号
    pub line: Option<u32>,
}

impl StackFrame {
    /// 返回人类可读的函数名，如果没有符号则显示地址
    pub fn display_name(&self) -> String {
        match &self.function {
            Some(name) => name.clone(),
            None => format!("0x{:016x}", self.ip),
        }
    }
}
