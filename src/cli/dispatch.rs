use crate::*;

pub(crate) fn run_command(command: Command) -> Result<()> {
    match command {
        command @ (Command::Init(_)
        | Command::Check(_)
        | Command::Diff(_)
        | Command::Handoff(_)
        | Command::Open(_)
        | Command::Status(_)
        | Command::Readiness(_)
        | Command::Resolve(_)
        | Command::Expectations(_)
        | Command::Verify(_)) => run_project_command(command),
        Command::Review { args, command } => run_review_command(&args, command),
        command @ (Command::Attestation { .. }
        | Command::Expectation { .. }
        | Command::Verification { .. }
        | Command::Decision { .. }
        | Command::Work { .. }) => run_record_command(command),
        Command::Git { args, command } => run_git_command(&args, command),
    }
}

fn run_project_command(command: Command) -> Result<()> {
    match command {
        command @ (Command::Init(_)
        | Command::Check(_)
        | Command::Diff(_)
        | Command::Handoff(_)) => run_project_maintenance(command),
        command @ (Command::Open(_)
        | Command::Status(_)
        | Command::Readiness(_)
        | Command::Resolve(_)
        | Command::Expectations(_)
        | Command::Verify(_)) => run_project_navigation(command),
        _ => unreachable!("non-project command routed to project dispatcher"),
    }
}

fn run_project_maintenance(command: Command) -> Result<()> {
    match command {
        Command::Init(args) => init_repository(&args),
        Command::Check(args) => check(&args),
        Command::Diff(args) => diff(&args),
        Command::Handoff(args) => handoff(&args),
        _ => unreachable!("non-maintenance command routed to maintenance dispatcher"),
    }
}

fn run_project_navigation(command: Command) -> Result<()> {
    match command {
        Command::Open(args) => open_shortcut(&args),
        Command::Status(args) => status_shortcut(&args),
        Command::Readiness(args) => readiness_command::run(&args),
        Command::Resolve(args) => resolve_target(&args),
        Command::Expectations(args) => expectations_shortcut(&args),
        Command::Verify(args) => verify_shortcut(args),
        _ => unreachable!("non-navigation command routed to navigation dispatcher"),
    }
}

fn run_review_command(args: &ReviewShortcutArgs, command: Option<ReviewCommand>) -> Result<()> {
    match command {
        Some(ReviewCommand::Build(args)) => build_review(&args),
        Some(ReviewCommand::Create(args)) => create_review(&args),
        Some(ReviewCommand::Open(args)) => open_review(&args),
        Some(ReviewCommand::Diff(args)) => diff_reviews(&args),
        Some(ReviewCommand::Serve(args)) => serve_review(&args),
        Some(ReviewCommand::ExportHtml(args)) => export_review_html(&args),
        None => review_shortcut(args),
    }
}

fn run_record_command(command: Command) -> Result<()> {
    match command {
        command @ (Command::Attestation { .. } | Command::Expectation { .. }) => {
            run_authored_command(command)
        }
        command @ Command::Verification { .. } => run_verification_command(command),
        command @ (Command::Decision { .. } | Command::Work { .. }) => {
            run_decision_or_work_command(command)
        }
        _ => unreachable!("non-record command routed to record dispatcher"),
    }
}

fn run_authored_command(command: Command) -> Result<()> {
    match command {
        Command::Attestation { command } => match command {
            AttestationCommand::Inspect(args) => inspect_attestation(&args),
        },
        Command::Expectation { command } => match command {
            ExpectationCommand::Add(args) => add_expectation(args),
            ExpectationCommand::List(args) => list_expectations(&args),
            ExpectationCommand::Remove(args) => remove_expectation(&args),
        },
        _ => unreachable!("non-authored command routed to authored dispatcher"),
    }
}

fn run_verification_command(command: Command) -> Result<()> {
    match command {
        Command::Verification { command } => match command {
            VerificationCommand::Add(args) => add_verification(args),
            VerificationCommand::List(args) => list_verifications(&args),
            VerificationCommand::Remove(args) => remove_verification(&args),
            VerificationCommand::Chain(args) => verification_chain(&args),
        },
        _ => unreachable!("non-verification command routed to verification dispatcher"),
    }
}

fn run_decision_or_work_command(command: Command) -> Result<()> {
    match command {
        Command::Decision { command } => match command {
            DecisionCommand::Add(args) => add_decision(args),
            DecisionCommand::List(args) => list_decisions(&args),
            DecisionCommand::Remove(args) => remove_decision(&args),
        },
        Command::Work { command } => match command {
            WorkCommand::Add(args) => add_work(args),
            WorkCommand::List(args) => list_works(&args),
            WorkCommand::Remove(args) => remove_work(&args),
        },
        _ => unreachable!("non-decision/work command routed to dispatcher"),
    }
}

fn run_git_command(args: &GitShortcutArgs, command: Option<GitCommand>) -> Result<()> {
    match command {
        Some(GitCommand::Connect(args)) => git_connect(&args),
        Some(GitCommand::Link(args)) => git_link(&args),
        Some(GitCommand::Import(args)) => import_git_work(&args),
        Some(GitCommand::Rewind(args)) => git_rewind(&args),
        Some(GitCommand::Signature(args)) => inspect_git_signature(&args),
        None => git_shortcut(args),
    }
}
