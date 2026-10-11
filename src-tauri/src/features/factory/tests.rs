use super::{store, types::*};
use rusqlite::Connection;

fn specification() -> Specification {
    Specification {
        goals: vec!["Coordinated change".into()],
        exclusions: vec![],
        acceptance_criteria: vec!["Works across repositories".into()],
        repository_changes: vec![],
        checks: vec![FeatureCheck { repository_id: 1, command: "test -f \"$ISSUE_PILOT_REPO_1/feature.txt\" && test -f \"$ISSUE_PILOT_REPO_2/feature.txt\"".into() }],
        questions: vec![],
        tasks: vec![
            Task {
                id: "ui".into(),
                repository_id: 2,
                title: "UI".into(),
                instructions: "Consume API".into(),
                dependencies: vec!["api".into()],
                acceptance_criteria: vec!["Renders".into()],
            },
            Task {
                id: "api".into(),
                repository_id: 1,
                title: "API".into(),
                instructions: "Add endpoint".into(),
                dependencies: vec![],
                acceptance_criteria: vec!["Responds".into()],
            },
        ],
    }
}
#[test]
fn repository_contracts_do_not_expose_origin_credentials() {
    let origin = super::engine::sanitized_origin(
        "https://user:example-secret@github.com/owner/repo.git?token=example-secret#private",
    );
    assert_eq!(origin, "https://github.com/owner/repo.git");
    assert_eq!(
        super::engine::sanitized_origin("example-secret@github.com:owner/repo.git"),
        "github.com:owner/repo.git"
    );
}
#[test]
fn orders_cross_repository_dependencies() {
    assert_eq!(task_order(&specification(), &[1, 2]).unwrap(), vec![1, 0]);
}
#[test]
fn rejects_cycles_unknown_dependencies_and_repositories() {
    let mut spec = specification();
    spec.tasks[1].dependencies.push("ui".into());
    assert!(task_order(&spec, &[1, 2]).is_err());
    spec.tasks[1].dependencies = vec!["missing".into()];
    assert!(task_order(&spec, &[1, 2]).is_err());
    assert!(task_order(&specification(), &[1]).is_err());
}
#[test]
fn rejects_malformed_agent_output() {
    assert!(serde_json::from_str::<Review>(
        r#"{"summary":"ok","blockingFindings":[],"scopeChange":false,"extra":true}"#
    )
    .is_err());
}
#[test]
fn approved_revision_cannot_be_modified() {
    let db = Connection::open_in_memory().unwrap();
    store::migrate(&db).unwrap();
    store::migrate(&db).unwrap();
    db.execute(
        "INSERT INTO factory_revisions VALUES('revision','feature','{}')",
        [],
    )
    .unwrap();
    assert!(db
        .execute(
            "UPDATE factory_revisions SET document='changed' WHERE id='revision'",
            []
        )
        .is_err());
}

fn feature() -> Feature {
    Feature {
        id: "test-feature".into(),
        title: "Coordinated API and UI".into(),
        brief: "Support feature delivery".into(),
        source_issue_id: None,
        repository_ids: vec![1, 2],
        status: FeatureStatus::Blocked,
        messages: vec![],
        draft: None,
        approved_revision_id: Some("revision".into()),
        repositories: vec![],
        approvals: vec![],
        completed_tasks: vec![],
        repair_counts: Default::default(),
        feedback_cursors: Default::default(),
        runs: vec![],
        artifacts: vec![],
        validations: vec![],
        operations: vec![],
        events: vec![],
        error: None,
        paused: false,
        created_at: store::timestamp(),
    }
}
#[test]
fn repair_budget_survives_persistence() {
    let mut feature = feature();
    for _ in 0..3 {
        require_repair(&mut feature, "api").unwrap();
    }
    let mut restored: Feature =
        serde_json::from_str(&serde_json::to_string(&feature).unwrap()).unwrap();
    assert!(require_repair(&mut restored, "api").is_err());
    assert!(require_repair(&mut restored, "integration").is_ok());
}
#[test]
fn restart_requires_explicit_resume_and_preserves_legacy_tables() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE checkpoints(job_id TEXT PRIMARY KEY); INSERT INTO checkpoints VALUES('legacy');").unwrap();
    store::migrate(&db).unwrap();
    let mut feature = feature();
    feature.status = FeatureStatus::Implementing;
    store::write(&db, &feature).unwrap();
    store::migrate(&db).unwrap();
    let restored = store::read(&db, &feature.id).unwrap();
    assert_eq!(restored.status, FeatureStatus::Blocked);
    assert!(restored.paused);
    assert!(restored.error.is_some());
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn shared_scheduler_excludes_issues_and_other_factory_stages() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let state = crate::AppState {
            db: std::sync::Arc::new(tokio::sync::Mutex::new(
                Connection::open_in_memory().unwrap(),
            )),
            running: std::sync::Arc::new(tokio::sync::Mutex::new(true)),
            factory_control: Default::default(),
            factory_pid: Default::default(),
            active_feature: Default::default(),
            test_repositories: None,
        };
        assert!(super::acquire(&state, "feature").await.is_err());
        *state.running.lock().await = false;
        super::acquire(&state, "feature").await.unwrap();
        assert!(super::acquire(&state, "other").await.is_err());
        let waiting = state.clone();
        let waiter =
            tokio::spawn(async move { crate::scheduler::reserve(&waiting.running, true).await });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        super::release(&state).await;
        assert!(waiter.await.unwrap());
        assert!(*state.running.lock().await);
        *state.running.lock().await = false;
    });
}

#[cfg(unix)]
mod integration {
    use super::*;
    use crate::factory::{delivery, engine, process};
    use std::{
        path::{Path, PathBuf},
        sync::{atomic::Ordering, Arc},
    };
    use tokio::sync::Mutex;

    struct Fixture {
        root: PathBuf,
        state: crate::AppState,
        context: store::Context,
        feature: Feature,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().into()
    }
    fn executable(path: &Path, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn make_fixture() -> Fixture {
        let root =
            std::env::temp_dir().join(format!("issue-pilot-factory-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE repo_links(repository_id INTEGER PRIMARY KEY,local_path TEXT,base_branch TEXT,test_command TEXT);")
            .unwrap();
        store::migrate(&db).unwrap();
        let codex = root.join("fake-codex");
        executable(
            &codex,
            r#"#!/usr/bin/env python3
import sys,json,pathlib
args=sys.argv[1:]
answer=pathlib.Path(args[args.index('--output-last-message')+1])
prompt=sys.stdin.read() if args[-1]=='-' else args[-1]
if prompt.startswith('Implement'):
    if 'repair_feedback' in prompt: pathlib.Path('feedback.txt').write_text('Applied feedback')
    path=pathlib.Path('feature.txt')
    if 'failed' in prompt: path.write_text('repaired')
    elif not path.exists(): path.write_text('initial')
result={'summary':'Validated response','blockingFindings':[],'scopeChange':False}
if prompt.startswith('Act as'):
    pathlib.Path(__file__).with_name('planner-sandbox').write_text(args[args.index('--sandbox')+1])
    result={'goals':['Coordinated change'],'exclusions':['No deployment'],'acceptanceCriteria':['Works across repositories'],'repositoryChanges':['API and UI'], 'tasks':[{'id':'api','repositoryId':1,'title':'API','instructions':'Add endpoint','dependencies':[],'acceptanceCriteria':['Responds']},{'id':'ui','repositoryId':2,'title':'UI','instructions':'Consume API','dependencies':['api'],'acceptanceCriteria':['Renders']}], 'checks':[{'repositoryId':1,'command':'test -f "$ISSUE_PILOT_REPO_1/feature.txt" && test -f "$ISSUE_PILOT_REPO_2/feature.txt"'}], 'questions':[] if 'Use HTTP' in prompt else ['Which transport should be used?']}
if prompt.startswith('Classify'):
    counter=pathlib.Path(__file__).with_name('classify-count')
    counter.write_text(str(int(counter.read_text())+1 if counter.exists() else 1))
    if 'expand_scope' in prompt: result['scopeChange']=True
    elif 'repair_feedback' in prompt: result['blockingFindings']=['repair_feedback']
answer.write_text(json.dumps(result))
"#,
        );
        let gh = root.join("fake-gh");
        let script = r#"#!/usr/bin/env python3
import sys,json,pathlib,subprocess
root=pathlib.Path(__file__).parent
key=pathlib.Path.cwd().name
state=root/(key+'.pr.json')
args=sys.argv[1:]
if args[:2]==['pr','list']:
    if state.exists():
        pr=json.loads(state.read_text())
        pr['headRefOid']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
        print(json.dumps([pr]))
    else: print('[]')
elif args[:2]==['pr','create']:
    count=root/'create-count'
    count.write_text(str(int(count.read_text())+1 if count.exists() else 1))
    state.write_text(json.dumps({'url':'https://github.com/example/repo/pull/'+key,'headRefOid':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'isDraft':True,'state':'OPEN','baseRefName':'main'}))
    if (root/'lose-create').exists():
        (root/'lose-create').unlink()
        sys.exit(1)
    print('https://github.com/example/repo/pull/'+key)
elif args[:2]==['pr','ready']:
    pr=json.loads(state.read_text());pr['isDraft']='--undo' in args;state.write_text(json.dumps(pr))
    if (root/'lose-ready').exists():
        (root/'lose-ready').unlink();sys.exit(1)
elif args[:2]==['repo','view']: print('example/repo')
elif args[:2]==['pr','view']: print('1')
elif args[0]=='api':
    comments=root/'comments.json'
    print(comments.read_text() if comments.exists() and '/issues/' in args[-1] else '[[]]')
"#;
        executable(&gh, script);
        crate::put(&db, "codex_path", codex.to_str().unwrap()).unwrap();
        crate::put(&db, "gh_path", gh.to_str().unwrap()).unwrap();
        let mut feature = feature();
        for id in [1, 2] {
            let remote = root.join(format!("remote-{id}.git"));
            let checkout = root.join(format!("repo-{id}"));
            std::fs::create_dir_all(&checkout).unwrap();
            git(&root, &["init", "--bare", remote.to_str().unwrap()]);
            git(&checkout, &["init", "-b", "main"]);
            git(&checkout, &["config", "user.name", "Factory Test"]);
            git(&checkout, &["config", "user.email", "factory@example.test"]);
            std::fs::write(checkout.join("README.md"), "Fixture").unwrap();
            git(&checkout, &["add", "README.md"]);
            git(&checkout, &["commit", "-m", "Initial"]);
            git(
                &checkout,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            );
            git(&checkout, &["push", "-u", "origin", "main"]);
            let sha = git(&checkout, &["rev-parse", "HEAD"]);
            db.execute(
                "INSERT INTO repo_links VALUES(?1,?2,'main',?3)",
                rusqlite::params![
                    id,
                    checkout.display().to_string(),
                    "test \"$(cat feature.txt)\" = repaired"
                ],
            )
            .unwrap();
            feature.repositories.push(RepositorySnapshot {
                repository_id: id,
                local_path: checkout.display().to_string(),
                base_branch: "main".into(),
                base_sha: sha,
                test_command: "test \"$(cat feature.txt)\" = repaired".into(),
                remote: remote.display().to_string(),
                branch: format!("issue-pilot/feature-test-{id}"),
                worktree: root.join(format!("worktree-{id}")).display().to_string(),
                head_sha: None,
                validated_tree: None,
                pr_url: None,
                ready: false,
            });
        }
        let spec = specification();
        let revision = FeatureRevision {
            id: "revision".into(),
            specification: spec,
            repositories: feature.repositories.clone(),
        };
        feature.draft = Some(revision.clone());
        db.execute(
            "INSERT INTO factory_revisions VALUES(?1,?2,?3)",
            rusqlite::params![
                revision.id,
                feature.id,
                serde_json::to_string(&revision).unwrap()
            ],
        )
        .unwrap();
        store::write(&db, &feature).unwrap();
        let state = crate::AppState {
            db: Arc::new(Mutex::new(db)),
            running: Default::default(),
            factory_control: Default::default(),
            factory_pid: Default::default(),
            active_feature: Default::default(),
            test_repositories: Some(vec![
                serde_json::json!({"id":1,"active":true,"access_status":"active","full_name":"example/repo"}),
                serde_json::json!({"id":2,"active":true,"access_status":"active","full_name":"example/repo"}),
            ]),
        };
        let context = store::Context {
            directory: root.clone(),
            app: None,
        };
        Fixture {
            root,
            state,
            context,
            feature,
        }
    }

    #[test]
    fn two_repository_delivery_repairs_and_reconciles_lost_responses() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            std::fs::write(fixture.root.join("lose-create"), "yes").unwrap();
            assert!(
                engine::execute(&fixture.state, &fixture.context, &mut fixture.feature)
                    .await
                    .is_err()
            );
            assert_eq!(fixture.feature.completed_tasks, vec!["api", "ui"]);
            assert_eq!(fixture.feature.repair_counts.get("task:api"), Some(&1));
            assert_eq!(fixture.feature.repair_counts.get("task:ui"), Some(&1));
            engine::execute(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap();
            assert_eq!(
                fixture.feature.status,
                FeatureStatus::AwaitingDeliveryApproval
            );
            assert_eq!(
                std::fs::read_to_string(fixture.root.join("create-count")).unwrap(),
                "2"
            );
            assert!(fixture
                .feature
                .repositories
                .iter()
                .all(|repo| repo.pr_url.is_some()));
            assert!(fixture
                .feature
                .validations
                .iter()
                .any(|check| check.command.contains("ISSUE_PILOT_REPO_2") && check.passed));
            let shas = fixture
                .feature
                .repositories
                .iter()
                .filter_map(|repo| repo.head_sha.clone())
                .collect::<Vec<_>>();
            assert!(super::super::approvals::delivery(
                &mut fixture.feature,
                "wrong-revision".into(),
                shas.clone()
            )
            .is_err());
            assert!(super::super::approvals::delivery(
                &mut fixture.feature,
                "revision".into(),
                vec!["wrong-sha".into()]
            )
            .is_err());
            super::super::approvals::delivery(&mut fixture.feature, "revision".into(), shas)
                .unwrap();
            std::fs::write(fixture.root.join("lose-ready"), "yes").unwrap();
            assert!(
                delivery::deliver(&fixture.state, &fixture.context, &mut fixture.feature)
                    .await
                    .is_err()
            );
            delivery::deliver(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap();
            assert_eq!(fixture.feature.status, FeatureStatus::Delivered);
            assert!(fixture.feature.repositories.iter().all(|repo| repo.ready));
            assert_eq!(
                std::fs::read_to_string(fixture.root.join("create-count")).unwrap(),
                "2"
            );
        });
    }

    #[test]
    fn unavailable_tools_and_cancelled_child_produce_saved_failure_evidence() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            let result = process::run(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                "missing tool",
                None,
                "/missing/factory-tool",
                &[],
                &fixture.root,
                &[],
            )
            .await;
            assert!(result.is_err());
            assert_eq!(fixture.feature.runs.last().unwrap().status, "failed");
            let state = fixture.state.clone();
            let cancel = tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                state.factory_control.store(2, Ordering::SeqCst);
            });
            let result = process::run(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                "long process",
                None,
                "sh",
                &[
                    "-c".into(),
                    "sleep 20 & echo $! > sleep-child.pid; wait".into(),
                ],
                &fixture.root,
                &[],
            )
            .await;
            cancel.await.unwrap();
            let pid = std::fs::read_to_string(fixture.root.join("sleep-child.pid")).unwrap();
            let child = std::process::Command::new("ps")
                .args(["-p", pid.trim(), "-o", "stat="])
                .output()
                .unwrap();
            let status = String::from_utf8_lossy(&child.stdout);
            let alive = !status.trim().is_empty() && !status.trim().starts_with('Z');
            if alive {
                let _ = std::process::Command::new("kill")
                    .args(["-KILL", pid.trim()])
                    .status();
            }
            assert!(!alive, "Cancellation left a descendant process running");
            assert!(result.is_err());
            assert_eq!(fixture.feature.runs.last().unwrap().status, "failed");
            fixture.state.factory_control.store(0, Ordering::SeqCst);
            fixture
                .state
                .db
                .lock()
                .await
                .execute_batch("PRAGMA query_only=ON")
                .unwrap();
            let result = process::run(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                "persist before execution",
                None,
                "sh",
                &["-c".into(), "touch forbidden-write".into()],
                &fixture.root,
                &[],
            )
            .await;
            assert!(result.is_err());
            assert!(!fixture.root.join("forbidden-write").exists());
        });
    }

    #[test]
    fn changed_base_and_worktree_heads_block_execution() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            let tools = engine::tools(&fixture.state).await;
            engine::verify_worktrees(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &tools,
            )
            .await
            .unwrap();
            let repo = fixture.feature.repositories[0].clone();
            git(
                Path::new(&repo.worktree),
                &["commit", "--allow-empty", "-m", "Unexpected"],
            );
            assert!(engine::verify_worktrees(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &tools
            )
            .await
            .is_err());
            let mut other = make_fixture();
            let repo = other.feature.repositories[0].clone();
            git(
                Path::new(&repo.local_path),
                &["commit", "--allow-empty", "-m", "Changed base"],
            );
            assert!(
                engine::execute(&other.state, &other.context, &mut other.feature)
                    .await
                    .is_err()
            );
            let mut remote = make_fixture();
            let repo = remote.feature.repositories[0].clone();
            let cwd = Path::new(&repo.local_path);
            let tree = git(cwd, &["rev-parse", "HEAD^{tree}"]);
            let sha = git(
                cwd,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &repo.base_sha,
                    "-m",
                    "Remote advanced",
                ],
            );
            git(cwd, &["push", "origin", &format!("{sha}:refs/heads/main")]);
            let error = engine::execute(&remote.state, &remote.context, &mut remote.feature)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("Origin base branch"));
        });
    }
    #[test]
    fn repair_exhaustion_blocks_publication_and_scope_change_invalidates_approval() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            fixture.feature.repositories[0].test_command = "false".into();
            let mut revision = fixture.feature.draft.clone().unwrap();
            revision.repositories = fixture.feature.repositories.clone();
            // Use an independent approved revision rather than mutate the immutable snapshot.
            revision.id = "failing-revision".into();
            fixture.feature.approved_revision_id = Some(revision.id.clone());
            {
                let db = fixture.state.db.lock().await;
                db.execute(
                    "INSERT INTO factory_revisions VALUES(?1,?2,?3)",
                    rusqlite::params![
                        revision.id,
                        fixture.feature.id,
                        serde_json::to_string(&revision).unwrap()
                    ],
                )
                .unwrap();
            }
            let error = engine::execute(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("Three repair cycles exhausted"));
            assert_eq!(fixture.feature.repair_counts.get("task:api"), Some(&3));
            assert!(!fixture.root.join("create-count").exists());

            let mut scope = make_fixture();
            let codex = scope.root.join("fake-codex");
            let source = std::fs::read_to_string(&codex)
                .unwrap()
                .replace("'scopeChange':False", "'scopeChange':True");
            executable(&codex, &source);
            assert!(
                engine::execute(&scope.state, &scope.context, &mut scope.feature)
                    .await
                    .is_err()
            );
            assert!(scope.feature.approved_revision_id.is_none());
            let db = scope.state.db.lock().await;
            assert!(store::approved(&db, &scope.feature).is_err());
        });
    }

    #[test]
    fn interrupted_factory_commit_is_reconciled_but_manual_edits_block_delivery() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            let tools = engine::tools(&fixture.state).await;
            engine::verify_worktrees(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &tools,
            )
            .await
            .unwrap();
            let repo = fixture.feature.repositories[0].clone();
            fixture.feature.repositories[0].validated_tree = Some(git(
                Path::new(&repo.worktree),
                &["rev-parse", "HEAD^{tree}"],
            ));
            fixture.feature.operations.push(Operation {
                repository_id: 1,
                branch: repo.branch.clone(),
                kind: "commit:API".into(),
                sha: repo.base_sha.clone(),
                completed: false,
            });
            git(
                Path::new(&repo.worktree),
                &["commit", "--allow-empty", "-m", "Feature test-feature: API"],
            );
            engine::verify_worktrees(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &tools,
            )
            .await
            .unwrap();
            assert!(fixture.feature.operations[0].completed);
            assert_ne!(
                fixture.feature.repositories[0].head_sha.as_ref(),
                Some(&repo.base_sha)
            );
            std::fs::write(
                Path::new(&repo.worktree).join("manual.txt"),
                "Unexpected post-validation change",
            )
            .unwrap();
            assert!(delivery::verify_clean(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &tools
            )
            .await
            .is_err());
        });
    }
    #[test]
    fn planner_inspects_multiple_repositories_and_requires_answers_before_execution() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            fixture.feature.approved_revision_id = None;
            super::super::planning::propose(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap();
            assert_eq!(
                fixture
                    .feature
                    .draft
                    .as_ref()
                    .unwrap()
                    .specification
                    .questions
                    .len(),
                1
            );
            let unresolved_id = fixture.feature.draft.as_ref().unwrap().id.clone();
            assert!(super::super::approvals::specification(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &unresolved_id
            )
            .await
            .is_err());
            assert_eq!(
                fixture.feature.status,
                FeatureStatus::AwaitingSpecificationApproval
            );
            assert!(
                engine::execute(&fixture.state, &fixture.context, &mut fixture.feature)
                    .await
                    .is_err()
            );
            fixture.feature.messages.push(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "user".into(),
                text: "Use HTTP".into(),
                created_at: store::timestamp(),
            });
            super::super::planning::propose(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap();
            let draft = fixture.feature.draft.as_ref().unwrap();
            assert!(draft.specification.questions.is_empty());
            assert_eq!(draft.repositories.len(), 2);
            assert_eq!(
                std::fs::read_to_string(fixture.root.join("planner-sandbox")).unwrap(),
                "read-only"
            );
            assert!(!fixture.root.join("create-count").exists());
            let revision_id = fixture.feature.draft.as_ref().unwrap().id.clone();
            assert!(super::super::approvals::specification(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                "stale-revision"
            )
            .await
            .is_err());
            super::super::approvals::specification(
                &fixture.state,
                &fixture.context,
                &mut fixture.feature,
                &revision_id,
            )
            .await
            .unwrap();
            assert_eq!(
                fixture.feature.approved_revision_id.as_ref(),
                Some(&revision_id)
            );
            super::super::planning::propose(&fixture.state, &fixture.context, &mut fixture.feature)
                .await
                .unwrap();
            assert!(fixture.feature.approved_revision_id.is_none());
            let db = fixture.state.db.lock().await;
            assert!(store::approved(&db, &fixture.feature).is_err());
            let retained: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM factory_revisions WHERE id=?)",
                    [&revision_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(retained);
        });
    }

    #[test]
    fn feedback_is_deduplicated_repaired_and_scope_expansion_requires_new_approval() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut fixture = make_fixture();
            engine::execute(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            let shas = fixture.feature.repositories.iter().filter_map(|repo| repo.head_sha.clone()).collect::<Vec<_>>();
            super::super::approvals::delivery(&mut fixture.feature, "revision".into(), shas).unwrap();
            delivery::deliver(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            std::fs::write(fixture.root.join("comments.json"), r#"[[{"id":7,"updated_at":"2026-10-11T12:00:00Z","body":"repair_feedback","html_url":"https://github.com/example/repo/pull/1#comment-7","user":{"login":"reviewer"}}]]"#).unwrap();
            super::super::feedback::sync(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            assert_eq!(fixture.feature.status, FeatureStatus::Blocked);
            assert_eq!(fixture.feature.feedback_cursors.len(), 2);
            super::super::feedback::sync(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            assert_eq!(std::fs::read_to_string(fixture.root.join("classify-count")).unwrap(), "1");
            let edited = std::fs::read_to_string(fixture.root.join("comments.json")).unwrap().replace("repair_feedback", "repair_feedback edited");
            std::fs::write(fixture.root.join("comments.json"), edited).unwrap();
            super::super::feedback::sync(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            assert_eq!(std::fs::read_to_string(fixture.root.join("classify-count")).unwrap(), "2");
            engine::execute(&fixture.state, &fixture.context, &mut fixture.feature).await.unwrap();
            assert_eq!(fixture.feature.status, FeatureStatus::AwaitingDeliveryApproval);
            assert!(fixture.feature.repositories.iter().all(|repo| !repo.ready));
            assert!(fixture.feature.repositories.iter().all(|repo| Path::new(&repo.worktree).join("feedback.txt").exists()));
            super::super::feedback::classify(&fixture.state, &fixture.context, &mut fixture.feature, "expand_scope".into()).await.unwrap();
            assert!(fixture.feature.approved_revision_id.is_none());
            assert_eq!(fixture.feature.status, FeatureStatus::AwaitingSpecificationApproval);
        });
    }
}
