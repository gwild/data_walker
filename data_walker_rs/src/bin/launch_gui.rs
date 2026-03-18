use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use sysinfo::System;

fn main() -> Result<(), Box<dyn Error>> {
    let wrapper_exe = env::current_exe()?;
    let profile_dir = parent_dir(&wrapper_exe, "wrapper executable directory")?;
    let target_dir = parent_dir(profile_dir, "target directory")?;
    let crate_dir = parent_dir(target_dir, "crate directory")?;

    let profile_name = profile_name(profile_dir)?;
    let gui_exe_path = profile_dir.join(executable_name("data_walker"));

    terminate_existing_gui_instances(&gui_exe_path)?;
    build_gui_binary(crate_dir, profile_name)?;
    run_pre_spawn_hook(&gui_exe_path)?;
    spawn_gui(&gui_exe_path)?;

    Ok(())
}

fn parent_dir<'a>(path: &'a Path, label: &str) -> Result<&'a Path, Box<dyn Error>> {
    path.parent()
        .ok_or_else(|| format!("Missing {}", label).into())
}

fn profile_name(profile_dir: &Path) -> Result<&str, Box<dyn Error>> {
    let name = profile_dir
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| "Unable to determine cargo profile directory".to_string())?;

    if name == "debug" || name == "release" {
        Ok(name)
    } else {
        Err(format!("Unsupported cargo profile directory '{}'", name).into())
    }
}

fn executable_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

fn run_pre_spawn_hook(gui_exe_path: &Path) -> Result<(), Box<dyn Error>> {
    terminate_existing_gui_instances(gui_exe_path)
}

fn terminate_existing_gui_instances(gui_exe_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut system = System::new_all();
    system.refresh_all();

    for process in system.processes().values() {
        if is_matching_gui_process(process, gui_exe_path) {
            process
                .kill_and_wait()
                .map_err(|error| format!("Failed to stop existing GUI process: {error:?}"))?;
        }
    }

    Ok(())
}

fn is_matching_gui_process(process: &sysinfo::Process, gui_exe_path: &Path) -> bool {
    if process.exe() == Some(gui_exe_path) {
        return true;
    }

    match (process.exe(), gui_exe_path.file_name()) {
        (Some(process_path), Some(gui_file_name)) => process_path
            .file_name()
            .map(|process_file_name| {
                process_file_name
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&gui_file_name.to_string_lossy())
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn build_gui_binary(crate_dir: &Path, profile_name: &str) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new("cargo");
    command.current_dir(crate_dir);
    command.arg("build");
    command.arg("--bin");
    command.arg("data_walker");

    if profile_name == "release" {
        command.arg("--release");
    }

    let status = command.status()?;
    if !status.success() {
        return Err("cargo build --bin data_walker failed".into());
    }

    Ok(())
}

fn spawn_gui(gui_exe_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(gui_exe_path);
    command.arg("gui");

    for arg in env::args_os().skip(1) {
        command.arg(arg);
    }

    let _child = command.spawn()?;
    Ok(())
}
