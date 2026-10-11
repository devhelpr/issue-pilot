use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    Drafting,
    Planning,
    AwaitingSpecificationApproval,
    Queued,
    Implementing,
    Validating,
    AwaitingDeliveryApproval,
    Delivered,
    Blocked,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub repository_id: i64,
    pub title: String,
    pub instructions: String,
    pub dependencies: Vec<String>,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureCheck {
    pub repository_id: i64,
    pub command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Specification {
    pub goals: Vec<String>,
    pub exclusions: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub repository_changes: Vec<String>,
    pub tasks: Vec<Task>,
    pub checks: Vec<FeatureCheck>,
    pub questions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySnapshot {
    pub repository_id: i64,
    pub local_path: String,
    pub base_branch: String,
    pub base_sha: String,
    pub test_command: String,
    pub remote: String,
    pub branch: String,
    pub worktree: String,
    pub head_sha: Option<String>,
    #[serde(default)]
    pub validated_tree: Option<String>,
    pub pr_url: Option<String>,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureRevision {
    pub id: String,
    pub specification: Specification,
    pub repositories: Vec<RepositorySnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default = "new_id")]
    pub id: String,
    pub role: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub kind: String,
    pub revision_id: String,
    pub commit_shas: Vec<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub role: String,
    pub task_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub log_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub run_id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    #[serde(default = "new_id")]
    pub id: String,
    pub task_id: Option<String>,
    pub repository_id: i64,
    pub command: String,
    pub passed: bool,
    pub output: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub repository_id: i64,
    #[serde(default)]
    pub branch: String,
    pub kind: String,
    pub sha: String,
    pub completed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub id: String,
    pub title: String,
    pub brief: String,
    pub source_issue_id: Option<i64>,
    pub repository_ids: Vec<i64>,
    pub status: FeatureStatus,
    pub messages: Vec<Message>,
    pub draft: Option<FeatureRevision>,
    pub approved_revision_id: Option<String>,
    pub repositories: Vec<RepositorySnapshot>,
    pub approvals: Vec<Approval>,
    pub completed_tasks: Vec<String>,
    pub repair_counts: std::collections::BTreeMap<String, u8>,
    #[serde(default)]
    pub feedback_cursors: std::collections::BTreeMap<String, String>,
    pub runs: Vec<Run>,
    pub artifacts: Vec<Artifact>,
    pub validations: Vec<ValidationResult>,
    pub operations: Vec<Operation>,
    pub events: Vec<Message>,
    pub error: Option<String>,
    pub paused: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateFeature {
    pub title: String,
    pub brief: String,
    pub repository_ids: Vec<i64>,
    pub source_issue_id: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub summary: String,
    pub blocking_findings: Vec<String>,
    pub scope_change: bool,
}

pub fn task_order(spec: &Specification, repository_ids: &[i64]) -> Result<Vec<usize>> {
    if spec.goals.is_empty() || spec.acceptance_criteria.is_empty() || spec.tasks.is_empty() {
        bail!("A specification needs goals, acceptance criteria, and tasks");
    }
    if spec.checks.is_empty() {
        bail!("Add feature acceptance/integration commands before approval");
    }
    if spec
        .goals
        .iter()
        .chain(&spec.acceptance_criteria)
        .any(|text| text.trim().is_empty())
    {
        bail!("Goals and acceptance criteria cannot be blank");
    }
    let ids: HashSet<_> = spec.tasks.iter().map(|task| task.id.as_str()).collect();
    if ids.len() != spec.tasks.len() || ids.contains("") {
        bail!("Task IDs must be unique and nonempty");
    }
    for task in &spec.tasks {
        if !repository_ids.contains(&task.repository_id)
            || task.title.trim().is_empty()
            || task.instructions.trim().is_empty()
            || task.acceptance_criteria.is_empty()
        {
            bail!("Every task needs an approved repository, instructions and acceptance criteria");
        }
        if task
            .dependencies
            .iter()
            .any(|id| !ids.contains(id.as_str()))
        {
            bail!("Task references an unknown dependency");
        }
    }
    for check in &spec.checks {
        if !repository_ids.contains(&check.repository_id) || check.command.trim().is_empty() {
            bail!("Validation commands must name a selected repository and be nonempty");
        }
    }
    let mut order = Vec::new();
    let mut resolved = HashSet::new();
    while order.len() < spec.tasks.len() {
        let next = spec.tasks.iter().enumerate().find(|(_, task)| {
            !resolved.contains(&task.id) && task.dependencies.iter().all(|id| resolved.contains(id))
        });
        let (index, task) = next.ok_or_else(|| anyhow!("Task dependencies contain a cycle"))?;
        resolved.insert(task.id.clone());
        order.push(index);
    }
    Ok(order)
}

pub fn require_repair(feature: &mut Feature, key: &str) -> Result<()> {
    let count = feature.repair_counts.entry(key.into()).or_default();
    if *count >= 3 {
        bail!("Three repair cycles exhausted for {key}; inspect evidence and revise the plan");
    }
    *count += 1;
    Ok(())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
