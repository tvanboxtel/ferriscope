#[path = "../src/preview_label.rs"]
mod preview_label;

#[test]
fn formats_partial_progress() {
    assert_eq!(preview_label::preview_label(2, 5), "2 of 5 files reviewed");
}
