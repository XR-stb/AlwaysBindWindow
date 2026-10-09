//! Window selection is temporary until the user explicitly confirms binding.
use crate::group::{MovementMode, TrackedWindow};

pub struct PickerResult {
    pub selected_windows: Vec<TrackedWindow>,
    pub cancelled: bool,
    pub movement: MovementMode,
}

#[cfg(target_os = "windows")]
#[path = "overlay_windows.rs"]
mod win_impl;
#[cfg(target_os = "windows")]
pub use win_impl::run_picker_overlay;

#[cfg(not(target_os = "windows"))]
pub fn run_picker_overlay(_global_sync_move: bool) -> PickerResult {
    use crate::platform;
    use std::io::{self, Write, BufRead};

    let windows = platform::enumerate_windows();
    if windows.len() < 2 {
        println!("Need at least 2 windows.");
        return PickerResult { selected_windows: Vec::new(), cancelled: true, movement: Default::default() };
    }

    println!("\nAvailable windows:");
    for (i, w) in windows.iter().enumerate() {
        let title: String = if w.title.chars().count() > 40 {
            format!("{}...", w.title.chars().take(37).collect::<String>())
        } else { w.title.clone() };
        println!("  [{}] {} - {}", i + 1, w.process_name, title);
    }
    println!("\nEnter numbers to bind (comma-separated), e.g. 1,2,3:");
    print!("> ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().lock().read_line(&mut input).is_err() {
        return PickerResult { selected_windows: Vec::new(), cancelled: true, movement: Default::default() };
    }

    let indices: Vec<usize> = input.trim().split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= windows.len())
        .collect();

    if indices.len() < 2 {
        println!("Need at least 2 windows.");
        return PickerResult { selected_windows: Vec::new(), cancelled: true, movement: Default::default() };
    }

    let selected: Vec<crate::group::TrackedWindow> = indices.iter()
        .map(|&i| windows[i - 1].clone())
        .collect();

    PickerResult { selected_windows: selected, cancelled: false, movement: Default::default() }
}
