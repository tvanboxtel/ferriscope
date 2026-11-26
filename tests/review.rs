use ferriscope::*;

#[test]
fn line_counts_use_github_totals_even_without_a_patch() {
    let file = |counts: serde_json::Value| {
        let mut json = serde_json::json!({"filename":"large.rs","status":"modified","sha":"same"});
        json.as_object_mut()
            .unwrap()
            .extend(counts.as_object().unwrap().clone());
        serde_json::from_value::<File>(json).unwrap()
    };
    let with_counts = file(serde_json::json!({"additions": 712, "deletions": 101}));
    let without_counts = file(serde_json::json!({}));
    assert!(with_counts.patch().is_none());
    assert_eq!(with_counts.line_changes(), Some((712, 101)));
    assert_eq!(without_counts.line_changes(), None);
    assert_eq!(with_counts.fingerprint(), without_counts.fingerprint());
}

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
    files[2] = serde_json::from_value(serde_json::json!({
        "filename":"tests/accounts.rs","sha":"new","status":"added",
        "patch":"@@ -0,0 +1 @@\n+#[test] fn rejects_missing_account() {}"
    }))
    .unwrap();
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
