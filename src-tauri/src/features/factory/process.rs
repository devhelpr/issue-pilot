use super::{store, types::*};
use crate::AppState;
use anyhow::{anyhow, bail, Result};
use std::{path::Path, process::Stdio, sync::atomic::Ordering, time::Duration};
use tokio::process::Command;
use uuid::Uuid;

pub fn boundary(state: &AppState) -> Result<()> {
    match state.factory_control.load(Ordering::SeqCst) {
        1 => bail!("Paused at a safe boundary"),
        2 => bail!("Feature cancelled"),
        _ => Ok(()),
    }
}

pub async fn run(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    role: &str,
    task_id: Option<&str>,
    bin: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> Result<(i32, String)> {
    run_with_input(
        state, app, feature, role, task_id, bin, args, cwd, env, None,
    )
    .await
}

async fn run_with_input(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    role: &str,
    task_id: Option<&str>,
    bin: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    input: Option<&Path>,
) -> Result<(i32, String)> {
    boundary(state)?;
    let id = Uuid::new_v4().to_string();
    let dir = app.directory.clone().join("factory").join(&feature.id);
    std::fs::create_dir_all(&dir)?;
    let log_path = dir.join(format!("{id}.log"));
    let log = std::fs::File::create(&log_path)?;
    feature.runs.push(Run {
        id: id.clone(),
        role: role.into(),
        task_id: task_id.map(String::from),
        status: "running".into(),
        created_at: store::timestamp(),
        log_path: log_path.display().to_string(),
    });
    store::event(feature, format!("Started {role}"));
    store::save(state, app, feature).await?;
    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .envs(env.iter().cloned())
        .stdin(match input {
            Some(path) => Stdio::from(std::fs::File::open(path)?),
            None => Stdio::null(),
        })
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let outcome: Result<i32> = async {
        let mut child = command
            .spawn()
            .map_err(|error| anyhow!("Could not start {role} ({bin}): {error}"))?;
        state
            .factory_pid
            .store(child.id().unwrap_or(0), Ordering::SeqCst);
        let started = std::time::Instant::now();
        loop {
            if let Ok(status) = tokio::time::timeout(Duration::from_millis(150), child.wait()).await
            {
                return Ok(status?.code().unwrap_or(-1));
            }
            if state.factory_control.load(Ordering::SeqCst) == 2
                || started.elapsed() > Duration::from_secs(1800)
            {
                if let Some(pid) = child.id() {
                    terminate_process_group(pid);
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
                bail!("{role} cancelled or exceeded the 30 minute timeout");
            }
        }
    }
    .await;
    terminate_process_group(state.factory_pid.swap(0, Ordering::SeqCst));
    use std::io::{Read, Seek, SeekFrom};
    let mut log = std::fs::File::open(&log_path)?;
    let length = log.metadata()?.len();
    log.seek(SeekFrom::Start(length.saturating_sub(4_000_000)))?;
    let mut bytes = Vec::new();
    log.take(4_000_000).read_to_end(&mut bytes)?;
    let mut output = String::from_utf8_lossy(&bytes).to_string();
    if length > 4_000_000 {
        output.insert_str(0, "[Earlier output retained in the process log]\n");
    }
    if let Err(error) = &outcome {
        output.push_str(&format!("\n{error}"));
    }
    if let Some(run) = feature.runs.iter_mut().find(|run| run.id == id) {
        run.status = match &outcome {
            Ok(0) => "succeeded",
            _ => "failed",
        }
        .into();
    }
    feature.artifacts.push(Artifact {
        run_id: id,
        kind: role.into(),
        text: crate::truncated_to(&output, 32_000),
    });
    store::save(state, app, feature).await?;
    Ok((outcome?, output))
}

pub async fn checked(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    role: &str,
    bin: &str,
    args: &[&str],
    cwd: &Path,
) -> Result<String> {
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    let (code, output) = run(state, app, feature, role, None, bin, &args, cwd, &[]).await?;
    if code != 0 {
        bail!("{role} failed (exit {code}): {output}");
    }
    Ok(output.trim().into())
}

pub async fn agent<T: serde::de::DeserializeOwned>(
    state: &AppState,
    app: &store::Context,
    feature: &mut Feature,
    role: &str,
    task_id: Option<&str>,
    cwd: &Path,
    prompt: String,
    schema: serde_json::Value,
    read_only: bool,
) -> Result<T> {
    let codex = {
        let db = state.db.lock().await;
        crate::tool(&db, "codex_path", "codex")
    };
    let dir = app.directory.clone().join("factory").join(&feature.id);
    std::fs::create_dir_all(&dir)?;
    let id = Uuid::new_v4();
    let schema_path = dir.join(format!("{id}.schema.json"));
    let answer_path = dir.join(format!("{id}.answer.json"));
    let prompt_path = dir.join(format!("{id}.prompt.txt"));
    std::fs::write(&prompt_path, prompt)?;
    std::fs::write(&schema_path, serde_json::to_vec(&schema)?)?;
    let args = vec![
        "exec".into(),
        "--sandbox".into(),
        if read_only {
            "read-only"
        } else {
            "workspace-write"
        }
        .into(),
        "--output-schema".into(),
        schema_path.display().to_string(),
        "--output-last-message".into(),
        answer_path.display().to_string(),
        "-".into(),
    ];
    let (code, _) = run_with_input(
        state,
        app,
        feature,
        role,
        task_id,
        &codex,
        &args,
        cwd,
        &[],
        Some(&prompt_path),
    )
    .await?;
    if code != 0 {
        bail!("{role} failed; inspect the saved process log");
    }
    if std::fs::metadata(&answer_path)?.len() > 1_000_000 {
        bail!("{role} response exceeded the 1 MB structured-output limit");
    }
    let answer = std::fs::read_to_string(answer_path)
        .map_err(|_| anyhow!("{role} did not produce a structured answer"))?;
    feature.artifacts.push(Artifact {
        run_id: feature
            .runs
            .last()
            .map(|run| run.id.clone())
            .unwrap_or_default(),
        kind: format!("{role}_answer"),
        text: answer.clone(),
    });
    store::save(state, app, feature).await?;
    serde_json::from_str(&answer).map_err(|error| {
        anyhow!("Invalid {role} output: {error}; structured response retained in evidence")
    })
}

/// Each factory child starts a new process group so cancellation also stops its descendants.
pub fn terminate_process_group(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
