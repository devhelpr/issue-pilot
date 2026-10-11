use super::types::*;
use crate::AppState;
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use tauri::{Emitter, Manager};

#[derive(Clone)]
pub struct Context {
    pub directory: std::path::PathBuf,
    pub app: Option<tauri::AppHandle>,
}
impl Context {
    pub fn new(app: tauri::AppHandle) -> Result<Self> {
        Ok(Self {
            directory: app.path().app_data_dir()?,
            app: Some(app),
        })
    }
    pub fn emit(&self, id: &str) {
        if let Some(app) = &self.app {
            let _ = app.emit("factory-progress", id);
        }
    }
}

pub fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

pub fn migrate(db: &Connection) -> Result<()> {
    db.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS factory_migrations(version INTEGER PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS factory_features(id TEXT PRIMARY KEY, document TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS factory_revisions(id TEXT PRIMARY KEY, feature_id TEXT NOT NULL, document TEXT NOT NULL);
         CREATE TRIGGER IF NOT EXISTS factory_revision_immutable BEFORE UPDATE ON factory_revisions
         BEGIN SELECT RAISE(ABORT, 'Approved revisions are immutable'); END;
         INSERT OR IGNORE INTO factory_migrations(version) VALUES(1);
         COMMIT;",
    )?;
    // Only explicit resume may restart a process after application shutdown.
    for mut feature in list(db)? {
        if matches!(
            feature.status,
            FeatureStatus::Planning
                | FeatureStatus::Queued
                | FeatureStatus::Implementing
                | FeatureStatus::Validating
        ) {
            feature.status = FeatureStatus::Blocked;
            feature.paused = true;
            feature.error = Some("Application stopped during execution. Inspect retained worktrees and resume explicitly.".into());
        }
        for run in &mut feature.runs {
            if run.status == "running" {
                run.status = "interrupted".into();
            }
        }
        write(db, &feature)?;
    }
    Ok(())
}

pub fn list(db: &Connection) -> Result<Vec<Feature>> {
    let mut statement = db.prepare("SELECT document FROM factory_features ORDER BY rowid DESC")?;
    let documents = statement.query_map([], |row| row.get::<_, String>(0))?;
    documents
        .map(|document| Ok(serde_json::from_str(&document?)?))
        .collect()
}

pub fn read(db: &Connection, id: &str) -> Result<Feature> {
    let document: String = db.query_row(
        "SELECT document FROM factory_features WHERE id=?",
        [id],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&document)?)
}

pub fn write(db: &Connection, feature: &Feature) -> Result<()> {
    db.execute("INSERT INTO factory_features(id,document) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET document=excluded.document", params![feature.id, serde_json::to_string(feature)?])?;
    Ok(())
}

pub async fn load(state: &AppState, id: &str) -> Result<Feature> {
    read(&*state.db.lock().await, id)
}

pub async fn save(state: &AppState, app: &Context, feature: &Feature) -> Result<()> {
    write(&*state.db.lock().await, feature)?;
    app.emit(&feature.id);
    Ok(())
}

pub fn event(feature: &mut Feature, text: impl Into<String>) {
    feature.events.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "system".into(),
        text: crate::truncated_to(text.into(), 8_000),
        created_at: timestamp(),
    });
}

pub fn approved(db: &Connection, feature: &Feature) -> Result<FeatureRevision> {
    let Some(id) = &feature.approved_revision_id else {
        bail!("Approve a specification before execution")
    };
    let document: String = db.query_row(
        "SELECT document FROM factory_revisions WHERE id=? AND feature_id=?",
        params![id, feature.id],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&document)?)
}
