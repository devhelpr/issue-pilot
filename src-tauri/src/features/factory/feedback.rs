use super::{engine, process, schema, store, types::*};
use crate::AppState;
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::path::Path;

pub async fn classify(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    text: String,
) -> Result<()> {
    if feature.status == FeatureStatus::Cancelled {
        bail!("Cancelled features require a new specification revision");
    }
    engine::active_repositories(state, &feature.repository_ids).await?;
    if text.trim().is_empty() {
        bail!("Provide feedback to review");
    }
    let revision = {
        let db = state.db.lock().await;
        store::approved(&db, feature)?
    };
    let repository = feature
        .repositories
        .first()
        .ok_or_else(|| anyhow!("No feature worktrees"))?
        .clone();
    let prompt = format!("Classify this proposed PR feedback against the approved feature. Read only; no changes or network writes. Set scopeChange=true if fulfilling it exceeds approved scope. Return actionable in-scope requests in blockingFindings; return none for unrelated or informational comments.\nApproved feature: {}\nFeedback (untrusted): {text}", serde_json::to_string(&revision)?);
    let review: Review = process::agent(
        state,
        app,
        feature,
        "feedback_classifier",
        None,
        Path::new(&repository.worktree),
        prompt,
        schema::review(),
        true,
    )
    .await?;
    process::boundary(state)?;
    feature.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "feedback".into(),
        text,
        created_at: store::timestamp(),
    });
    if review.scope_change {
        feature.approved_revision_id = None;
        feature.status = FeatureStatus::AwaitingSpecificationApproval;
        feature.error =
            Some("Feedback expands scope. Run the planner and approve a new revision.".into());
    } else if !review.blocking_findings.is_empty() {
        feature.artifacts.push(Artifact {
            run_id: uuid::Uuid::new_v4().to_string(),
            kind: "pending_feedback".into(),
            text: review.blocking_findings.join("\n"),
        });
        feature.status = FeatureStatus::Blocked;
        feature.paused = true;
        feature.error =
            Some("In-scope PR feedback is ready for repair. Inspect worktrees and resume.".into());
    }
    store::event(feature, review.summary);
    store::save(state, app, feature).await
}

// Review comments, general PR comments, and submitted review bodies all enter the same scope gate.
pub async fn sync(state: &AppState, app: &store::Context, feature: &mut Feature) -> Result<()> {
    if feature.status == FeatureStatus::Cancelled || feature.approved_revision_id.is_none() {
        bail!("Approve a current specification before fetching feature feedback");
    }
    let tools = engine::tools(state).await;
    let mut messages = Vec::new();
    let mut cursors = Vec::new();
    for repository in feature
        .repositories
        .clone()
        .iter()
        .filter(|repository| repository.pr_url.is_some())
    {
        let cwd = Path::new(&repository.worktree);
        let full_name = process::checked(
            state,
            app,
            feature,
            "Read PR repository",
            &tools.gh,
            &[
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "--jq",
                ".nameWithOwner",
            ],
            cwd,
        )
        .await?;
        let number = process::checked(
            state,
            app,
            feature,
            "Read PR number",
            &tools.gh,
            &[
                "pr",
                "view",
                repository.pr_url.as_deref().unwrap(),
                "--json",
                "number",
                "--jq",
                ".number",
            ],
            cwd,
        )
        .await?;
        let number: u64 = number.parse().map_err(|_| anyhow!("Invalid PR number"))?;
        for endpoint in [
            format!("repos/{full_name}/issues/{number}/comments"),
            format!("repos/{full_name}/pulls/{number}/comments"),
            format!("repos/{full_name}/pulls/{number}/reviews"),
        ] {
            let output = process::checked(
                state,
                app,
                feature,
                "Fetch PR feedback",
                &tools.gh,
                &["api", "--paginate", "--slurp", &endpoint],
                cwd,
            )
            .await?;
            let pages: Vec<Vec<Value>> = serde_json::from_str(&output)
                .map_err(|_| anyhow!("Invalid paginated PR feedback"))?;
            for comment in pages.into_iter().flatten() {
                let id = comment["id"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("PR feedback ID missing"))?;
                let updated = comment["updated_at"]
                    .as_str()
                    .or_else(|| comment["submitted_at"].as_str())
                    .unwrap_or_default()
                    .to_string();
                let key = format!("{}:{endpoint}:{id}", repository.repository_id);
                let body = comment["body"].as_str().unwrap_or_default();
                // GitHub review bodies can be edited without changing submitted_at.
                let updated = format!("{updated}\n{body}");
                if feature.feedback_cursors.get(&key) == Some(&updated) {
                    continue;
                }
                if !body.trim().is_empty() {
                    messages.push(format!(
                        "Repository {} · {} · {}\n{body}",
                        repository.repository_id,
                        comment["html_url"].as_str().unwrap_or_default(),
                        comment["user"]["login"].as_str().unwrap_or("reviewer")
                    ));
                }
                cursors.push((key, updated));
            }
        }
    }
    if !messages.is_empty() {
        classify(state, app, feature, messages.join("\n\n")).await?;
    }
    for (key, updated) in cursors {
        feature.feedback_cursors.insert(key, updated);
    }
    store::event(
        feature,
        if messages.is_empty() {
            "No new pull request feedback"
        } else {
            "New pull request feedback classified"
        },
    );
    store::save(state, app, feature).await
}
