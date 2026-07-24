use super::test_support::*;
use super::*;

#[test]
fn git_connect_marks_workflow_context_without_work_record_as_needs_record() {
    let artifact = test_artifact();
    let commit = test_commit("Touch checkout route", "", &["src/api.ts"]);

    let report = build_git_connect_report(&artifact, &[commit]);

    assert_eq!(report.connected, 0);
    assert_eq!(report.needs_record, 1);
    assert_eq!(report.unconnected, 0);
    assert_eq!(report.records[0].status, "needs_record");
    assert_eq!(report.records[0].workflows[0].id, "w_checkout");
    assert_eq!(report.records[0].expectations[0].id, "e_checkout_sequence");
    assert!(report.records[0].works.is_empty());
}

#[test]
fn git_connect_marks_commit_evidence_work_as_connected() {
    let mut artifact = test_artifact();
    artifact.works.push(Work {
        id: "wk_git_f240cd96a07f2ea7".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "import:git".to_owned(),
        evidence: Some("commit:f240cd96a07f2ea7".to_owned()),
        title: "Address checkout sequence".to_owned(),
        detail: "Imported from Git.".to_owned(),
    });
    let commit = test_commit("Address checkout sequence", "", &["src/api.ts"]);

    let report = build_git_connect_report(&artifact, std::slice::from_ref(&commit));

    assert_eq!(report.connected, 1);
    assert_eq!(report.needs_record, 0);
    assert_eq!(report.unconnected, 0);
    assert_eq!(report.records[0].status, "connected");
    assert_eq!(report.records[0].works[0].id, "wk_git_f240cd96a07f2ea7");
}

#[test]
fn git_shortcut_merges_work_sidecar_before_connecting() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_path = temp.path().join("project.susu");
    let work_path = temp.path().join("work.susu");
    let artifact = test_artifact();
    let commit = test_commit("Address checkout sequence", "", &["src/api.ts"]);
    let work = Work {
        id: "wk_git_f240cd96a07f2ea7".to_owned(),
        target: ExpectationTarget::Workflow,
        subject: Some("w_checkout".to_owned()),
        expectation_id: Some("e_checkout_sequence".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "human:git-link".to_owned(),
        evidence: Some(format!("commit:{}", commit.hash)),
        title: "Address checkout sequence".to_owned(),
        detail: "Linked from git shortcut sidecar.".to_owned(),
    };
    fs::write(
        &artifact_path,
        write_susu(&artifact, false).expect("write artifact text"),
    )
    .expect("write artifact");
    fs::write(
        &work_path,
        write_works(&[work], false).expect("write work text"),
    )
    .expect("write work");
    let args = GitShortcutArgs {
        repo: temp.path().to_path_buf(),
        artifact: artifact_path,
        output: work_path,
        since: None,
        until: None,
        limit: 25,
        max_items: 20,
        no_export: false,
        source: "import:git-connect".to_owned(),
        minify: false,
        json: false,
    };

    let loaded = git_shortcut_artifact(&args).expect("load shortcut artifact");
    let report = build_git_connect_report(&loaded, &[commit]);

    assert_eq!(report.connected, 1);
    assert_eq!(report.records[0].status, "connected");
    assert_eq!(report.records[0].works[0].id, "wk_git_f240cd96a07f2ea7");
}

#[test]
fn git_connect_suggests_link_commands_for_unconnected_commits() {
    let mut artifact = test_artifact();
    artifact.expectations.push(Expectation {
        id: "e_docs_workflow".to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Docs guide daily project review".to_owned(),
        detail: "Documentation should explain routine review commands for project work.".to_owned(),
    });
    artifact.expectations.push(Expectation {
        id: "e_docs_commands".to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        status: ExpectationStatus::Accepted,
        source: "human:test".to_owned(),
        title: "Docs guide routine commands".to_owned(),
        detail: "Docs should explain local commands.".to_owned(),
    });
    let commit = test_commit("docs: update guide commands", "", &["README.md"]);

    let report = build_git_connect_report(&artifact, &[commit]);

    assert_eq!(report.records[0].status, "unconnected");
    assert!(
        report.records[0]
            .suggestions
            .iter()
            .any(|suggestion| suggestion.expectation_id == "e_docs_workflow")
    );
    assert!(
        report.records[0]
            .suggestions
            .iter()
            .any(|suggestion| suggestion.command == "susumu git link f240cd96 e_docs_workflow")
    );
}

#[test]
fn suggested_expectations_are_ranked_and_limited() {
    let mut artifact = test_artifact();
    artifact.expectations = vec![
        Expectation {
            id: "e_docs".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Docs guide daily workflow commands".to_owned(),
            detail: "Docs should explain review commands and link commands.".to_owned(),
        },
        Expectation {
            id: "e_git".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Guide suggests next commands".to_owned(),
            detail: "Output should suggest workflow commands.".to_owned(),
        },
        Expectation {
            id: "e_portal".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Daily status guide is visible".to_owned(),
            detail: "Review context and commands should stay visible.".to_owned(),
        },
        Expectation {
            id: "e_ai".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "AI output remains optional".to_owned(),
            detail: "AI generated summaries should be optional.".to_owned(),
        },
    ];

    let suggestions = crate::git::connect::suggested_expectations(
        &artifact,
        "docs: guide daily workflow commands",
    );

    assert_eq!(suggestions.len(), 3);
    assert_eq!(suggestions[0].id, "e_docs");
    assert!(
        suggestions[0].score >= suggestions[1].score,
        "suggestions should be score-ranked"
    );
}

#[test]
fn git_connect_export_uses_single_expectation_target() {
    let artifact = test_artifact();
    let commit = test_commit("Address e_checkout_sequence", "", &["notes.txt"]);
    let report = build_git_connect_report(&artifact, &[commit]);

    let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
    let work = &works[0];

    assert_eq!(works.len(), 1);
    assert_eq!(work.id, "wk_git_f240cd96a07f2ea7");
    assert_eq!(work.target, ExpectationTarget::Workflow);
    assert_eq!(work.subject.as_deref(), Some("w_checkout"));
    assert_eq!(work.expectation_id.as_deref(), Some("e_checkout_sequence"));
    assert_eq!(
        work.evidence.as_deref(),
        Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575")
    );
}

#[test]
fn git_connect_export_links_project_expectation_from_language_match() {
    let mut artifact = test_artifact();
    artifact.expectations = vec![
        Expectation {
            id: "e_expectation_support".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Expectations show supporting evidence".to_owned(),
            detail: "Review packets should show support for expectations.".to_owned(),
        },
        Expectation {
            id: "e_git_work_support".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Git work can support project expectations".to_owned(),
            detail: "Local Git commits should become work support for project expectations."
                .to_owned(),
        },
    ];
    let commit = test_commit(
        "feat: connect git work to project expectations",
        "",
        &["src/main.rs"],
    );
    let report = build_git_connect_report(&artifact, &[commit]);

    assert_eq!(report.needs_record, 1);
    assert_eq!(report.records[0].expectations[0].id, "e_git_work_support");

    let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
    let work = &works[0];

    assert_eq!(works.len(), 1);
    assert_eq!(work.target, ExpectationTarget::Project);
    assert_eq!(work.subject, None);
    assert_eq!(work.expectation_id.as_deref(), Some("e_git_work_support"));
    assert_eq!(work.source, "import:test");
    assert_eq!(
        work.evidence.as_deref(),
        Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575")
    );
    assert!(work.detail.contains("Generated by git connect."));

    artifact.works.extend(works);
    let support = expectation_support(&artifact);
    let git_support = support
        .iter()
        .find(|item| item.expectation_id == "e_git_work_support")
        .expect("git work support");

    assert_eq!(git_support.work, 1);
    assert_eq!(git_support.support_status, "partially_supported");
    assert!(
        git_support
            .reasons
            .iter()
            .any(|reason| reason == "1 linked work record(s)")
    );
}

#[test]
fn git_connect_export_writes_work_for_each_matched_expectation() {
    let mut artifact = test_artifact();
    artifact.expectations = vec![
        Expectation {
            id: "e_readiness".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Readiness board exists".to_owned(),
            detail: "Portal should show readiness.".to_owned(),
        },
        Expectation {
            id: "e_dirty_links".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Dirty source links exist".to_owned(),
            detail: "Portal should show dirty source links.".to_owned(),
        },
    ];
    let commit = test_commit(
        "feat: support e_readiness and e_dirty_links",
        "",
        &["src/main.rs"],
    );
    let report = build_git_connect_report(&artifact, std::slice::from_ref(&commit));

    assert_eq!(report.needs_record, 1);
    assert_eq!(report.records[0].expectations.len(), 2);
    assert!(
        report.records[0]
            .reasons
            .iter()
            .any(|reason| reason == "2 expectation work record(s) missing")
    );

    let works = works_from_git_connection(&artifact, &report.records[0], "import:test");

    assert_eq!(works.len(), 2);
    assert_ne!(works[0].id, works[1].id);
    assert_eq!(works[0].target, ExpectationTarget::Project);
    assert_eq!(works[1].target, ExpectationTarget::Project);
    assert_eq!(works[0].expectation_id.as_deref(), Some("e_dirty_links"));
    assert_eq!(works[1].expectation_id.as_deref(), Some("e_readiness"));

    artifact.works.push(Work {
        id: "wk_existing_dirty".to_owned(),
        target: ExpectationTarget::Project,
        subject: None,
        expectation_id: Some("e_dirty_links".to_owned()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: "human:test".to_owned(),
        evidence: Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575".to_owned()),
        title: "Existing dirty links work".to_owned(),
        detail: "Already connected.".to_owned(),
    });
    let report = build_git_connect_report(&artifact, &[commit]);
    let works = works_from_git_connection(&artifact, &report.records[0], "import:test");

    assert_eq!(report.needs_record, 1);
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].expectation_id.as_deref(), Some("e_readiness"));
}

#[test]
fn git_import_links_project_expectation_from_language_match() {
    let mut artifact = test_artifact();
    artifact.expectations = vec![
        Expectation {
            id: "e_expectation_support".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Expectations show supporting evidence".to_owned(),
            detail: "Review packets should show support for expectations.".to_owned(),
        },
        Expectation {
            id: "e_git_work_support".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:maintainer".to_owned(),
            title: "Git work can support project expectations".to_owned(),
            detail: "Local Git commits should become work support for project expectations."
                .to_owned(),
        },
    ];
    let commit = test_commit(
        "feat: connect git work to project expectations",
        "",
        &["src/main.rs"],
    );

    let linked = linked_git_expectation(&commit, Some(&artifact))
        .expect("language match should link exactly one expectation");

    assert_eq!(linked.id, "e_git_work_support");
    assert_eq!(linked.target, ExpectationTarget::Project);
    assert_eq!(linked.subject, None);

    let imported = imported_git_work(
        &commit,
        "import:test",
        &GitImportContext {
            target_depth: GitTargetDepth::Project,
            artifact: Some(&artifact),
        },
    );

    assert_eq!(
        imported.work.expectation_id.as_deref(),
        Some("e_git_work_support")
    );
    assert_eq!(imported.work.target, ExpectationTarget::Project);
    assert!(imported.targeting.contains("Linked exact expectation id"));
}

#[test]
fn git_connect_export_uses_single_workflow_when_expectation_is_ambiguous() {
    let artifact = test_artifact();
    let commit = test_commit("Touch checkout route", "", &["src/api.ts"]);
    let report = build_git_connect_report(&artifact, &[commit]);

    let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
    let work = &works[0];

    assert_eq!(work.target, ExpectationTarget::Workflow);
    assert_eq!(work.target, ExpectationTarget::Workflow);
    assert_eq!(work.subject.as_deref(), Some("w_checkout"));
    assert_eq!(work.expectation_id.as_deref(), Some("e_checkout_sequence"));
    assert!(work.detail.contains("Generated by git connect."));
    assert!(work.detail.contains("Changed files:"));
}

#[test]
fn git_import_targets_single_workflow_from_changed_files() {
    let artifact = test_artifact();
    let commit = test_commit("Touch checkout route", "", &["src/routes.php"]);
    let context = GitImportContext {
        artifact: Some(&artifact),
        target_depth: GitTargetDepth::Workflow,
    };

    let target = git_work_target(&commit, &context);

    assert_eq!(target.target, ExpectationTarget::Workflow);
    assert_eq!(target.subject.as_deref(), Some("w_php_checkout"));
    assert_eq!(
        target.note,
        "Matched exactly one workflow from changed files."
    );
}

#[test]
fn git_import_file_depth_targets_single_file_from_changed_files() {
    let artifact = test_artifact();
    let commit = test_commit("Touch api file", "", &["src/api.ts"]);
    let context = GitImportContext {
        artifact: Some(&artifact),
        target_depth: GitTargetDepth::File,
    };

    let target = git_work_target(&commit, &context);

    assert_eq!(target.target, ExpectationTarget::File);
    assert_eq!(target.subject.as_deref(), Some("f_api"));
    assert_eq!(
        target.note,
        "Matched exactly one artifact file from changed files."
    );
}

#[test]
fn git_import_uses_exact_expectation_id_when_files_do_not_match() {
    let artifact = test_artifact();
    let commit = test_commit("Address e_checkout_sequence", "", &["notes.txt"]);
    let context = GitImportContext {
        artifact: Some(&artifact),
        target_depth: GitTargetDepth::Workflow,
    };

    let imported = imported_git_work(&commit, "import:git", &context);

    assert_eq!(imported.work.target, ExpectationTarget::Workflow);
    assert_eq!(imported.work.subject.as_deref(), Some("w_checkout"));
    assert_eq!(
        imported.work.expectation_id.as_deref(),
        Some("e_checkout_sequence")
    );
    assert!(imported.targeting.contains("used its target"));
}

#[test]
fn git_import_json_report_includes_agent_friendly_fields() {
    let artifact = test_artifact();
    let commit = test_commit(
        "Address e_checkout_sequence",
        "Implementation detail.",
        &["notes.txt"],
    );
    let context = GitImportContext {
        artifact: Some(&artifact),
        target_depth: GitTargetDepth::Workflow,
    };
    let imported = vec![imported_git_work(&commit, "import:git", &context)];

    let report = build_git_import_json(Path::new("work.susu"), &imported);

    assert_eq!(report.output, "work.susu");
    assert_eq!(report.imported, 1);
    assert_eq!(report.records[0].id, "wk_git_f240cd96a07f2ea7");
    assert_eq!(
        report.records[0].commit,
        "f240cd96a07f2ea7b14cc1932c58914ed0871575"
    );
    assert_eq!(report.records[0].target, "workflow");
    assert_eq!(report.records[0].subject, Some("w_checkout"));
    assert_eq!(report.records[0].expectation, Some("e_checkout_sequence"));
    assert_eq!(report.records[0].changed_files, &["notes.txt".to_owned()]);
}

#[test]
fn safe_snapshot_paths_stay_under_snapshot_root() {
    let path = safe_snapshot_path(Path::new("snapshot"), "./src\\api.ts").unwrap();

    assert_eq!(path, PathBuf::from("snapshot").join("src").join("api.ts"));
}

#[test]
fn safe_snapshot_paths_reject_traversal_and_absolute_paths() {
    assert!(safe_snapshot_path(Path::new("snapshot"), "../secret.rs").is_err());
    assert!(safe_snapshot_path(Path::new("snapshot"), "/secret.rs").is_err());
    assert!(safe_snapshot_path(Path::new("snapshot"), "C:/secret.rs").is_err());
    assert!(safe_snapshot_path(Path::new("snapshot"), "c:/secret.rs").is_err());
}

#[test]
fn file_expectation_paths_resolve_to_scanner_ids() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");

    let subject = resolve_file_subject(
        temp.path(),
        ExpectationTarget::File,
        Some(".\\main.rs".to_owned()),
    )
    .expect("resolve file path");

    assert_eq!(subject, Some("f_a4075800b4a04993".to_owned()));
}

#[test]
fn review_shortcut_accepts_output_shorthand() {
    let cli = Cli::try_parse_from(["susumu", "review", "-o", "build/review"])
        .expect("parse review output shorthand");
    let Command::Review {
        args,
        command: None,
    } = cli.command.expect("review command")
    else {
        panic!("expected review shortcut");
    };
    assert_eq!(args.output_dir, PathBuf::from("build/review"));
}
