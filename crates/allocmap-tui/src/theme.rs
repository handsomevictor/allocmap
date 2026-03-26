use ratatui::style::{Color, Modifier, Style};

/// AllocMap 颜色主题
pub struct Theme;

impl Theme {
    // 状态颜色
    pub fn heap_normal() -> Style {
        Style::default().fg(Color::Green)
    }
    pub fn heap_growing() -> Style {
        Style::default().fg(Color::Yellow)
    }
    pub fn heap_critical() -> Style {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }

    // 信息颜色
    pub fn info() -> Style {
        Style::default().fg(Color::Cyan)
    }
    pub fn label() -> Style {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    }
    pub fn dimmed() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    // 边框
    pub fn border() -> Style {
        Style::default().fg(Color::Blue)
    }
    pub fn border_focused() -> Style {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    // 热点列表
    pub fn hotspot_top() -> Style {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
    pub fn hotspot_mid() -> Style {
        Style::default().fg(Color::Yellow)
    }
    pub fn hotspot_low() -> Style {
        Style::default().fg(Color::Green)
    }

    // 进度条颜色（用于内存占比展示）
    pub fn bar_fill() -> Style {
        Style::default().fg(Color::Cyan)
    }

    /// 根据增长速率返回对应颜色
    pub fn for_growth_rate(bytes_per_sec: f64) -> Style {
        if bytes_per_sec > 10.0 * 1024.0 * 1024.0 {
            // >10MB/s: 红色警告
            Self::heap_critical()
        } else if bytes_per_sec > 1.0 * 1024.0 * 1024.0 {
            // >1MB/s: 黄色提醒
            Self::heap_growing()
        } else {
            // 正常
            Self::heap_normal()
        }
    }
}
