use super::{
    engine::{active_repositories, tools, verify_worktrees, Tools},
    process, store,
    types::*,
};
use crate::AppState;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestSnapshot {
    url: String,
    head_ref_oid: String,
    is_draft: bool,
    state: String,
    base_ref_name: String,
}

fn verify_pr(pr: &PullRequestSnapshot, repository: &RepositorySnapshot) -> Result<()> {
    if pr.state != "OPEN"
        || pr.base_ref_name != repository.base_branch
        || Some(pr.head_ref_oid.as_str()) != repository.head_sha.as_deref()
        || pr.url.is_empty()
    {
        bail!("PR is closed or its base/HEAD differs from the validated changes");
    }
    Ok(())
}
use std::path::{Path, PathBuf};

pub async fn verify_clean(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    tools: &Tools,
) -> Result<()> {
    verify_worktrees(state, app, feature, tools).await?;
    for repository in feature.repositories.clone() {
        let cwd = Path::new(&repository.worktree);
        let head = process::checked(
            state,
            app,
            feature,
            "Verify validated SHA",
            &tools.git,
            &["rev-parse", "HEAD"],
            cwd,
        )
        .await?;
        let status = process::checked(
            state,
            app,
            feature,
            "Verify clean worktree",
            &tools.git,
            &["status", "--porcelain"],
            cwd,
        )
        .await?;
        if Some(head) != repository.head_sha || !status.is_empty() {
            bail!("Worktree changed after validation; resume to validate again");
        }
    }
    Ok(())
}

async fn intent(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    repository: &RepositorySnapshot,
    kind: &str,
) -> Result<usize> {
    let sha = repository
        .head_sha
        .clone()
        .ok_or_else(|| anyhow!("No validated commit"))?;
    if let Some(index) = feature.operations.iter().position(|operation| {
        operation.repository_id == repository.repository_id
            && operation.branch == repository.branch
            && operation.kind == kind
            && operation.sha == sha
    }) {
        return Ok(index);
    }
    feature.operations.push(Operation {
        repository_id: repository.repository_id,
        branch: repository.branch.clone(),
        kind: kind.into(),
        sha,
        completed: false,
    });
    store::save(state, app, feature).await?;
    Ok(feature.operations.len() - 1)
}

async fn external_boundary(state: &AppState, feature: &Feature) -> Result<()> {
    process::boundary(state)?;
    active_repositories(state, &feature.repository_ids).await?;
    process::boundary(state)
}

async fn pr_snapshot(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    tools: &Tools,
    repository: &RepositorySnapshot,
) -> Result<Option<PullRequestSnapshot>> {
    let output = process::checked(
        state,
        app,
        feature,
        "Reconcile feature PR",
        &tools.gh,
        &[
            "pr",
            "list",
            "--state",
            "all",
            "--head",
            &repository.branch,
            "--json",
            "url,headRefOid,isDraft,state,baseRefName",
            "--limit",
            "100",
        ],
        Path::new(&repository.worktree),
    )
    .await?;
    let rows: Vec<PullRequestSnapshot> = serde_json::from_str(&output)?;
    if rows.len() > 1 {
        bail!("Multiple PRs found for a feature branch; inspect them before resuming");
    }
    Ok(rows.into_iter().next())
}

fn pr_body(feature: &Feature) -> String {
    let companions = feature
        .repositories
        .iter()
        .filter_map(|repository| repository.pr_url.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut commands = feature
        .repositories
        .iter()
        .map(|repository| (repository.repository_id, repository.test_command.as_str()))
        .collect::<Vec<_>>();
    if let Some(revision) = &feature.draft {
        if feature.approved_revision_id.as_ref() == Some(&revision.id) {
            commands.extend(
                revision
                    .specification
                    .checks
                    .iter()
                    .map(|check| (check.repository_id, check.command.as_str())),
            );
        }
    }
    let mut seen = std::collections::HashSet::new();
    let evidence = commands
        .into_iter()
        .filter(|key| seen.insert(*key))
        .filter_map(|(repository_id, command)| {
            feature.validations.iter().rev().find(|result| {
                result.task_id.is_none()
                    && result.repository_id == repository_id
                    && result.command == command
            })
        })
        .map(|result| {
            format!(
                "- Repository {}: `{}` — {}",
                result.repository_id,
                result.command,
                if result.passed { "passed" } else { "failed" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("## Feature\n{}\n\nFeature ID: {}\nApproved revision: {}\n\n## Companion pull requests\n{}\n\n## Validation evidence\n{}\n\nMerging remains a manual action.", feature.title, feature.id, feature.approved_revision_id.as_deref().unwrap_or_default(), companions, evidence)
}

pub async fn publish(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    tools: &Tools,
) -> Result<()> {
    let changed = feature.repositories.iter().any(|repository| {
        repository
            .head_sha
            .as_ref()
            .is_some_and(|sha| sha != &repository.base_sha)
    });
    if !changed {
        bail!("Feature produced no changes; inspect the specification and execution evidence");
    }
    for (index, repository) in feature.repositories.clone().iter().enumerate() {
        if repository.head_sha.as_ref() == Some(&repository.base_sha) {
            continue;
        }
        external_boundary(state, feature).await?;
        let cwd = Path::new(&repository.worktree);
        let push = intent(state, app, feature, repository, "push").await?;
        let refs = process::checked(
            state,
            app,
            feature,
            "Inspect remote feature branch",
            &tools.git,
            &[
                "ls-remote",
                "origin",
                &format!("refs/heads/{}", repository.branch),
            ],
            cwd,
        )
        .await?;
        let remote_sha = refs.split_whitespace().next();
        if remote_sha != repository.head_sha.as_deref() {
            if remote_sha.is_some()
                && !feature.operations.iter().any(|operation| {
                    operation.repository_id == repository.repository_id
                        && operation.branch == repository.branch
                        && operation.kind == "push"
                        && operation.completed
                        && Some(operation.sha.as_str()) == remote_sha
                })
            {
                bail!("Remote feature branch changed unexpectedly");
            }
            external_boundary(state, feature).await?;
            process::checked(
                state,
                app,
                feature,
                "Push feature branch",
                &tools.git,
                &["push", "--set-upstream", "origin", &repository.branch],
                cwd,
            )
            .await?;
        }
        feature.operations[push].completed = true;
        store::save(state, app, feature).await?;
        let create = intent(state, app, feature, repository, "create_pr").await?;
        let mut pr = pr_snapshot(state, app, feature, tools, repository).await?;
        if pr.is_none() {
            let body_path = write_body(app, feature)?;
            external_boundary(state, feature).await?;
            process::checked(
                state,
                app,
                feature,
                "Create feature draft PR",
                &tools.gh,
                &[
                    "pr",
                    "create",
                    "--draft",
                    "--head",
                    &repository.branch,
                    "--base",
                    &repository.base_branch,
                    "--title",
                    &feature.title.clone(),
                    "--body-file",
                    &body_path.display().to_string(),
                ],
                cwd,
            )
            .await?;
            pr = pr_snapshot(state, app, feature, tools, repository).await?;
        }
        let pr = pr.ok_or_else(|| anyhow!("PR creation could not be reconciled"))?;
        verify_pr(&pr, repository)?;
        feature.repositories[index].pr_url = Some(pr.url);
        feature.operations[create].completed = true;
        store::save(state, app, feature).await?;
    }
    for repository in feature
        .repositories
        .clone()
        .iter()
        .filter(|repository| repository.pr_url.is_some())
    {
        let operation = intent(state, app, feature, repository, "link_prs").await?;
        let body_path = write_body(app, feature)?;
        external_boundary(state, feature).await?;
        process::checked(
            state,
            app,
            feature,
            "Link companion PRs",
            &tools.gh,
            &[
                "pr",
                "edit",
                repository.pr_url.as_deref().unwrap(),
                "--body-file",
                &body_path.display().to_string(),
            ],
            Path::new(&repository.worktree),
        )
        .await?;
        feature.operations[operation].completed = true;
        store::save(state, app, feature).await?;
    }
    Ok(())
}

fn write_body(app: &store::Context, feature: &Feature) -> Result<PathBuf> {
    let path = app
        .directory
        .clone()
        .join("factory")
        .join(&feature.id)
        .join("pr-body.md");
    std::fs::write(&path, pr_body(feature))?;
    Ok(path)
}

pub async fn deliver(state: &AppState, app: &store::Context, feature: &mut Feature) -> Result<()> {
    super::approvals::require_delivery_approval(feature)?;
    let tools = tools(state).await;
    verify_clean(state, app, feature, &tools).await?;
    // Verify every PR before marking any ready. A partial delivery is retryable.
    for repository in feature
        .repositories
        .clone()
        .iter()
        .filter(|repository| repository.pr_url.is_some())
    {
        let pr = pr_snapshot(state, app, feature, &tools, repository)
            .await?
            .ok_or_else(|| anyhow!("Feature PR missing"))?;
        verify_pr(&pr, repository)?;
    }
    for (index, repository) in feature.repositories.clone().iter().enumerate() {
        if repository.pr_url.is_none() {
            continue;
        }
        let operation = intent(state, app, feature, repository, "ready_pr").await?;
        let pr = pr_snapshot(state, app, feature, &tools, repository)
            .await?
            .ok_or_else(|| anyhow!("PR missing"))?;
        verify_pr(&pr, repository)?;
        if pr.is_draft {
            external_boundary(state, feature).await?;
            process::checked(
                state,
                app,
                feature,
                "Mark PR ready for review",
                &tools.gh,
                &["pr", "ready", repository.pr_url.as_deref().unwrap()],
                Path::new(&repository.worktree),
            )
            .await?;
        }
        feature.repositories[index].ready = true;
        feature.operations[operation].completed = true;
        store::save(state, app, feature).await?;
    }
    process::boundary(state)?;
    feature.status = FeatureStatus::Delivered;
    feature.error = None;
    store::event(
        feature,
        "All feature PRs are ready for review. Merge manually.",
    );
    store::save(state, app, feature).await
}

pub async fn return_to_draft(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    tools: &Tools,
) -> Result<()> {
    for (index, repository) in feature.repositories.clone().iter().enumerate() {
        if repository.pr_url.is_none() {
            continue;
        }
        let operation = intent(state, app, feature, repository, "draft_again").await?;
        let pr = pr_snapshot(state, app, feature, tools, repository)
            .await?
            .ok_or_else(|| anyhow!("Feature PR missing"))?;
        verify_pr(&pr, repository)?;
        if !pr.is_draft {
            external_boundary(state, feature).await?;
            process::checked(
                state,
                app,
                feature,
                "Return PR to draft for feedback",
                &tools.gh,
                &[
                    "pr",
                    "ready",
                    repository.pr_url.as_deref().unwrap(),
                    "--undo",
                ],
                Path::new(&repository.worktree),
            )
            .await?;
        }
        feature.operations[operation].completed = true;
        feature.repositories[index].ready = false;
        store::save(state, app, feature).await?;
    }
    Ok(())
}
