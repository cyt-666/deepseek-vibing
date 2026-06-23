#[cfg(not(unix))]
fn main() {
    eprintln!("deepseek-vibing-execve-wrapper is only implemented for UNIX");
    std::process::exit(1);
}

#[cfg(unix)]
pub use deepseek_vibing_shell_escalation::main_execve_wrapper as main;
