//! Review sandbox only; intentionally not wired into the app.

pub fn preview_label(reviewed: usize, total: usize) -> String {
    format!("{reviewed} of {total} files reviewed")
}
