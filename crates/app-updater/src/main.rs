#![windows_subsystem = "windows"]
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--apply" {
        return std::process::ExitCode::FAILURE;
    }
    match lazybuddy_app_updater::apply_job(std::path::Path::new(&args[1])) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
