use ferriscope::*;

#[test]
fn reopen_then_refresh_only_the_test() {
    let mut files: Vec<File> = serde_json::from_str(include_str!("mixed.json")).unwrap();
    let mut state = Review::default();
    state.reconcile(&files);
    assert!(state.groups.iter().all(|g| g.files.len() == 1));
    let pair = [files[1].filename.clone(), files[2].filename.clone()];
    state
        .together("Account deletion", &pair.clone().into())
        .unwrap();
    state.groups[0].files.swap(0, 1);
    state.toggle(&files[1]);
    state.toggle(&files[2]);
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut resumed: Review = serde_json::from_slice(&bytes).unwrap();
    files[2].patch = Some("@@ -0,0 +1 @@\n+#[test] fn rejects_missing_account() {}".into());
    resumed.reconcile(&files);
    assert_eq!(resumed.groups[0].files, [pair[1].clone(), pair[0].clone()]);
    assert_eq!(
        resumed.groups.iter().map(|g| g.files.len()).sum::<usize>(),
        6
    );
    assert_eq!(resumed.status(&files[1]), "Reviewed");
    assert_eq!(resumed.status(&files[2]), "Changed");
    resumed.toggle(&files[2]);
    assert_eq!(resumed.status(&files[2]), "Reviewed");
}

#[test]
fn canonical_key_is_scoped_to_github() {
    assert_eq!(
        pr_key("https://github.com/Org/Repo/pull/12/files#diff-abc").unwrap(),
        "org/repo/pull/12"
    );
    assert!(pr_key("https://example.com/org/repo/pull/12").is_err());
    assert!(pr_key("https://github.com/../repo/pull/12").is_err());
}
