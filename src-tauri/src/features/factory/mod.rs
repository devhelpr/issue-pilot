mod approvals;
mod delivery;
mod engine;
mod feedback;
mod planning;
mod process;
mod schema;
mod store;
#[cfg(test)]
mod tests;
mod types;

use crate::AppState;
use anyhow::{anyhow, bail, Result};
pub use process::terminate_process_group;
use std::{path::Path, sync::atomic::Ordering};
pub use store::migrate;
use tauri::State;
use types::*;
use uuid::Uuid;

async fn acquire(state: &AppState, id: &str) -> Result<()> {
    if !crate::scheduler::reserve(&state.running, false).await {
        bail!("Runner is busy with another feature, issue job, or PR follow-up; retry after it finishes");
    }
    *state.active_feature.lock().await = Some(id.into());
    state.factory_control.store(0, Ordering::SeqCst);
    Ok(())
}
async fn release(state: &AppState) {
    let id = state.active_feature.lock().await.take();
    let control = state.factory_control.load(Ordering::SeqCst);
    if control != 0 {
        if let Some(id) = id {
            let db = state.db.lock().await;
            if let Ok(mut feature) = store::read(&db, &id) {
                if control == 2 {
                    feature.status = FeatureStatus::Cancelled;
                }
                feature.paused = control == 1;
                store::event(
                    &mut feature,
                    if control == 2 {
                        "Cancellation completed; worktrees retained"
                    } else {
                        "Paused at a safe boundary"
                    },
                );
                let _ = store::write(&db, &feature);
            }
        }
    }
    *state.running.lock().await = false;
}
fn editable(feature: &Feature) -> Result<()> {
    if matches!(
        feature.status,
        FeatureStatus::Planning
            | FeatureStatus::Queued
            | FeatureStatus::Implementing
            | FeatureStatus::Validating
    ) {
        bail!("Pause execution before editing a feature");
    }
    Ok(())
}
fn finish_error(state: &AppState, feature: &mut Feature, error: anyhow::Error) {
    feature.status = if state.factory_control.load(Ordering::SeqCst) == 2 {
        FeatureStatus::Cancelled
    } else {
        FeatureStatus::Blocked
    };
    feature.paused = state.factory_control.load(Ordering::SeqCst) == 1;
    feature.error = Some(error.to_string());
    store::event(feature, error.to_string());
}

#[tauri::command]
pub async fn list_features(state: State<'_, AppState>) -> Result<Vec<Feature>, String> {
    store::list(&*state.db.lock().await).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_feature(
    input: CreateFeature,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    async {
        if input.title.trim().is_empty()
            || input.brief.trim().is_empty()
            || input.repository_ids.is_empty()
        {
            bail!("Provide a title, brief, and at least one linked repository");
        }
        let ids: std::collections::HashSet<_> = input.repository_ids.iter().collect();
        if ids.len() != input.repository_ids.len() {
            bail!("Repository IDs must be unique");
        }
        let db = state.db.lock().await;
        for id in &input.repository_ids {
            let _: String = db.query_row(
                "SELECT local_path FROM repo_links WHERE repository_id=?",
                [id],
                |row| row.get(0),
            )?;
        }
        let feature = Feature {
            id: Uuid::new_v4().to_string(),
            title: input.title,
            brief: input.brief,
            source_issue_id: input.source_issue_id,
            repository_ids: input.repository_ids,
            status: FeatureStatus::Drafting,
            messages: Vec::new(),
            draft: None,
            approved_revision_id: None,
            repositories: Vec::new(),
            approvals: Vec::new(),
            completed_tasks: Vec::new(),
            repair_counts: Default::default(),
            feedback_cursors: Default::default(),
            runs: Vec::new(),
            artifacts: Vec::new(),
            validations: Vec::new(),
            operations: Vec::new(),
            events: Vec::new(),
            error: None,
            paused: false,
            created_at: store::timestamp(),
        };
        store::write(&db, &feature)?;
        drop(db);
        app.emit(&feature.id);
        Ok(feature)
    }
    .await
    .map_err(|error: anyhow::Error| error.to_string())
}

#[tauri::command]
pub async fn plan_feature(
    id: String,
    message: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        feature.status = FeatureStatus::Planning;
        feature.approved_revision_id = None;
        feature.error = None;
        feature.messages.push(Message {
            id: Uuid::new_v4().to_string(),
            role: "user".into(),
            text: message,
            created_at: store::timestamp(),
        });
        store::save(&state, &app, &feature).await?;
        let result = planning::propose(&state, &app, &mut feature).await;
        if let Err(error) = result {
            finish_error(&state, &mut feature, error);
        }
        store::save(&state, &app, &feature).await?;
        Ok(feature)
    }
    .await;
    release(&state).await;
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_feature_revision(
    id: String,
    specification: Specification,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        task_order(&specification, &feature.repository_ids)?;
        let mut draft = feature
            .draft
            .clone()
            .ok_or_else(|| anyhow!("Run the planner before editing the specification"))?;
        let revision_id = Uuid::new_v4().to_string();
        // New revisions receive fresh branch identities; retained worktrees and old PRs are preserved.
        for repository in &mut draft.repositories {
            repository.branch = format!(
                "issue-pilot/feature-{}-{}-{}",
                feature.id,
                repository.repository_id,
                &revision_id[..8]
            );
            repository.worktree = Path::new(&repository.local_path)
                .join(".issue-pilot-worktrees")
                .join(format!(
                    "feature-{}-{}-{}",
                    feature.id,
                    repository.repository_id,
                    &revision_id[..8]
                ))
                .display()
                .to_string();
            repository.head_sha = None;
            repository.validated_tree = None;
            repository.pr_url = None;
            repository.ready = false;
        }
        draft.id = revision_id;
        draft.specification = specification;
        feature.draft = Some(draft);
        feature.approved_revision_id = None;
        feature.status = FeatureStatus::AwaitingSpecificationApproval;
        feature.error = None;
        store::event(
            &mut feature,
            "Edited specification; execution needs new approval",
        );
        store::save(&state, &app, &feature).await?;
        Ok(feature)
    }
    .await;
    release(&state).await;
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn approve_feature_revision(
    id: String,
    revision_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        approvals::specification(&state, &app, &mut feature, &revision_id).await?;
        Ok(feature)
    }
    .await;
    release(&state).await;
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn execute_feature(
    id: String,
    inspected: bool,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        if feature.status == FeatureStatus::Cancelled {
            bail!("Cancelled features need a new specification revision");
        }
        if feature.status == FeatureStatus::Delivered {
            bail!("Use the feedback action for delivered features");
        }
        {
            let db = state.db.lock().await;
            store::approved(&db, &feature)?;
        }
        if feature.error.is_some() && !inspected {
            bail!("Inspect retained worktrees and acknowledge before resuming");
        }
        feature.status = FeatureStatus::Queued;
        feature.paused = false;
        feature.error = None;
        store::save(&state, &app, &feature).await?;
        Ok(feature)
    }
    .await;
    match result {
        Err(error) => {
            release(&state).await;
            Err(error.to_string())
        }
        Ok(mut feature) => {
            let state = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = engine::execute(&state, &app, &mut feature).await {
                    finish_error(&state, &mut feature, error);
                }
                let _ = store::save(&state, &app, &feature).await;
                release(&state).await;
            });
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn control_feature(
    id: String,
    action: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    if action != "pause" && action != "cancel" {
        return Err("Use pause or cancel".into());
    }
    let active = state.active_feature.lock().await;
    if active.as_ref() == Some(&id) {
        state
            .factory_control
            .store(if action == "pause" { 1 } else { 2 }, Ordering::SeqCst);
        return Ok(());
    }
    drop(active);
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result = async {
        let mut db = state.db.lock().await;
        let transaction = db.transaction().map_err(|error| error.to_string())?;
        let mut feature = store::read(&transaction, &id).map_err(|error| error.to_string())?;
        feature.status = if action == "cancel" {
            FeatureStatus::Cancelled
        } else {
            FeatureStatus::Blocked
        };
        feature.paused = action == "pause";
        store::event(&mut feature, format!("Feature {action} requested"));
        store::write(&transaction, &feature).map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        app.emit(&id);
        Ok(())
    }
    .await;
    release(&state).await;
    result
}

#[tauri::command]
pub async fn approve_feature_delivery(
    id: String,
    revision_id: String,
    commit_shas: Vec<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        approvals::delivery(&mut feature, revision_id, commit_shas)?;
        store::save(&state, &app, &feature).await?;
        Ok(feature)
    }
    .await;
    match result {
        Err(error) => {
            release(&state).await;
            Err(error.to_string())
        }
        Ok(mut feature) => {
            let state = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = delivery::deliver(&state, &app, &mut feature).await {
                    finish_error(&state, &mut feature, error);
                    if state.factory_control.load(Ordering::SeqCst) != 2 {
                        feature.status = FeatureStatus::AwaitingDeliveryApproval;
                    }
                }
                let _ = store::save(&state, &app, &feature).await;
                release(&state).await;
            });
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn feature_feedback(
    id: String,
    text: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        feedback::classify(&state, &app, &mut feature, text).await?;
        Ok(feature)
    }
    .await;
    release(&state).await;
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_feature_feedback(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Feature, String> {
    let app = store::Context::new(app).map_err(|error| error.to_string())?;
    acquire(&state, &id)
        .await
        .map_err(|error| error.to_string())?;
    let result: Result<Feature> = async {
        let mut feature = store::load(&state, &id).await?;
        editable(&feature)?;
        feedback::sync(&state, &app, &mut feature).await?;
        Ok(feature)
    }
    .await;
    release(&state).await;
    result.map_err(|error| error.to_string())
}
