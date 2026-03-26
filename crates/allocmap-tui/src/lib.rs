/// allocmap-tui：基于 ratatui 的终端 UI
///
/// 颜色约定：
/// - 绿色：正常状态（内存稳定）
/// - 黄色：内存增长中（>1MB/s）
/// - 红色：快速增长（>10MB/s）或可能泄漏
/// - 青色：信息性数据（PID、程序名等）
/// - 白色：普通文本

pub mod app;
pub mod theme;
pub mod timeline;
pub mod hotspot;
pub mod events;

pub use app::App;
pub use theme::Theme;
