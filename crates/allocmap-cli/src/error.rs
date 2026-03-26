use owo_colors::OwoColorize;

/// 打印格式化的错误信息并退出
pub fn print_error_and_exit(err: &anyhow::Error) -> ! {
    eprintln!("{} {}", "Error:".red().bold(), err);

    // 如果有错误链，打印完整链
    let mut source = err.source();
    while let Some(cause) = source {
        eprintln!("  {} {}", "Caused by:".dimmed(), cause);
        source = cause.source();
    }

    std::process::exit(1);
}
