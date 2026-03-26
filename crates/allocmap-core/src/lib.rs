/// allocmap-core：核心数据结构，无平台依赖
/// 所有其他 crate 都依赖本 crate
pub mod sample;
pub mod recording;
pub mod error;

pub use sample::{SampleFrame, AllocationSite, StackFrame};
pub use recording::{AllocMapRecording, RecordingHeader, RecordingFooter};
pub use error::CoreError;
