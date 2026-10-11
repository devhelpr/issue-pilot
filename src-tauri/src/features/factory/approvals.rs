use super::{engine, process, store, types::*};
use crate::AppState;
use anyhow::{anyhow, bail, Result};
use rusqlite::params;
use std::path::Path;

pub async fn specification(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    revision_id: &str,
) -> Result<()> {
    let revision = feature
        .draft
        .clone()
        .ok_or_else(|| anyhow!("No specification to approve"))?;
    if revision.id != revision_id {
        bail!("Specification changed; review the latest revision");
    }
    if feature.approved_revision_id.as_deref() == Some(revision_id) {
        return Ok(());
    }
    {
        let db = state.db.lock().await;
        let already: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM factory_revisions WHERE id=?)",
            [revision_id],
            |row| row.get(0),
        )?;
        if already {
            bail!(
                "Save or plan a new revision before reapproval; previous approval was invalidated"
            );
        }
    }
    if !revision.specification.questions.is_empty() {
        bail!("Resolve planner questions before approval");
    }
    task_order(&revision.specification, &feature.repository_ids)?;
    let tools = engine::tools(state).await;
    engine::active_repositories(state, &feature.repository_ids).await?;
    for repository in &revision.repositories {
        let root = Path::new(&repository.local_path);
        let base = process::checked(
            state,
            app,
            feature,
            "Check approval base",
            &tools.git,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", repository.base_branch),
            ],
            root,
        )
        .await?;
        if base != repository.base_sha {
            bail!("Base changed since planning; run the planner again");
        }
        engine::verify_remote_base(
            state,
            app,
            feature,
            &tools,
            root,
            &repository.base_branch,
            &base,
        )
        .await?;
    }
    process::boundary(state)?;
    feature.approved_revision_id = Some(revision.id.clone());
    feature.repositories = revision.repositories.clone();
    for artifact in &mut feature.artifacts {
        if artifact.kind == "pending_feedback" || artifact.kind == "applied_feedback" {
            artifact.kind = "historical_feedback".into();
        }
    }
    feature.completed_tasks.clear();
    feature.repair_counts.clear();
    feature.status = FeatureStatus::Blocked;
    feature.paused = true;
    feature.error = None;
    feature.approvals.push(Approval {
        kind: "specification".into(),
        revision_id: revision.id.clone(),
        commit_shas: revision
            .repositories
            .iter()
            .map(|repo| repo.base_sha.clone())
            .collect(),
        created_at: store::timestamp(),
    });
    store::event(
        feature,
        "Approved scope, base commits, shell checks, and draft PR publication. Ready to execute.",
    );
    let mut db = state.db.lock().await;
    let transaction = db.transaction()?;
    transaction.execute(
        "INSERT INTO factory_revisions(id,feature_id,document) VALUES(?,?,?)",
        params![revision.id, feature.id, serde_json::to_string(&revision)?],
    )?;
    store::write(&transaction, feature)?;
    transaction.commit()?;
    app.emit(&feature.id);
    Ok(())
}

pub fn delivery(
    feature: &mut Feature,
    revision_id: String,
    commit_shas: Vec<String>,
) -> Result<()> {
    if feature.status != FeatureStatus::AwaitingDeliveryApproval {
        bail!("Feature is not awaiting delivery approval");
    }
    let actual = feature
        .repositories
        .iter()
        .filter_map(|repo| repo.head_sha.clone())
        .collect::<Vec<_>>();
    if feature.approved_revision_id.as_ref() != Some(&revision_id)
        || actual != commit_shas
        || actual.len() != feature.repositories.len()
        || actual.is_empty()
    {
        bail!("Validated revision or commits changed; review again");
    }
    feature.paused = false;
    feature.error = None;
    feature.approvals.push(Approval {
        kind: "delivery".into(),
        revision_id,
        commit_shas,
        created_at: store::timestamp(),
    });
    Ok(())
}

pub fn require_delivery_approval(feature: &Feature) -> Result<()> {
    let shas = feature
        .repositories
        .iter()
        .filter_map(|repo| repo.head_sha.clone())
        .collect::<Vec<_>>();
    if !feature.approvals.iter().any(|approval| {
        approval.kind == "delivery"
            && Some(&approval.revision_id) == feature.approved_revision_id.as_ref()
            && approval.commit_shas == shas
    }) {
        bail!("Approve the current validated commits before marking PRs ready");
    }
    Ok(())
}
