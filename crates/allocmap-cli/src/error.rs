use owo_colors::OwoColorize;

/// Print a formatted error message and exit with code 1.
#[allow(dead_code)]
pub fn print_error_and_exit(err: &anyhow::Error) -> ! {
    eprintln!("{} {}", "Error:".red().bold(), err);

    // Print the full error chain if available.
    let mut source = err.source();
    while let Some(cause) = source {
        eprintln!("  {} {}", "Caused by:".dimmed(), cause);
        source = cause.source();
    }

    std::process::exit(1);
}
