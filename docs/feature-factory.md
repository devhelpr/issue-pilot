# Feature factory

Issue Pilot now supports a local feature workflow alongside the existing issue runner:

Brief → read-only planning → specification approval → implementation, checks and review → linked draft PRs → delivery approval → ready for review.

## Getting started

1. Configure the Worker and installed, authenticated Codex, Git, and GitHub CLI tools in Settings.
2. Enable repositories and link each to a distinct local checkout, base branch, and test command.
3. Open Features. Write a brief or import an issue, select all affected repositories, and create the feature. Import never approves execution.
4. Run the read-only planner. Answer its questions in the planning conversation and run it again. Inspect goals, exclusions, acceptance criteria, task dependencies, repository changes, and feature checks. The JSON editor supports precise specification changes.
5. Approve the saved revision. This confirms its repository base commits, configured test commands, additional shell checks, and permission to publish draft PRs. Start execution separately.
6. Inspect the execution timeline and evidence. When validation succeeds, review the linked draft PRs and approve the validated commits to mark them ready for review. Merge manually.

New specification revisions use fresh branch and worktree identities. Existing revisions, worktrees, and PRs remain available for inspection; the factory does not close or delete superseded PRs. A changed local or origin base branch requires replanning and approval.

## Execution and validation

Rust owns the execution boundary. Factory planning, implementation, review, issue jobs, and issue-comment follow-up share the same exclusive runner. Manual busy requests return a visible error and can be retried. Automatically followed issues wait for the slot before claiming their Worker job, so a feature run does not discard them. Execution does not silently start after an application restart.

Tasks execute in dependency order, with one retained worktree per repository. Implementers receive the approved specification, dependency completions, recent evidence, and companion worktree paths. Planner and reviewer runs use Codex's read-only sandbox; implementers use workspace-write. Rust owns commits, pushes, and PR actions. The CLI uses existing authentication and structured final output as described in the [official non-interactive documentation](https://learn.chatgpt.com/docs/non-interactive-mode).

Every task runs its repository checks and independent review. Final validation runs all repository checks, feature checks, and cross-repository review. Approved feature-check commands execute in their named repository worktree. For companion paths, use environment variables such as `ISSUE_PILOT_REPO_12` and `ISSUE_PILOT_REPO_34` (Windows shell syntax applies on Windows). Commands are shell-interpreted only after specification approval.

Failures permit at most three repair cycles per task and three during integration. Budgets survive restarts. Exhaustion, malformed agent output, unavailable tools, scope expansion, and unexpected Git state block progress with retained evidence. A new approved revision resets execution and its repair budgets.

## Persistence and recovery

Versioned factory tables live alongside existing tables in the app-data SQLite database. Approved revision snapshots cannot be updated. Feature documents store approvals, task completion, repair counts, runs, artifacts, validation results, external-operation intents, PR identities, and feedback cursors. Full process logs, prompt files, and structured agent responses are retained under the app-data `factory/<feature-id>/` directory.

Pause waits for the current process to finish and stops before the next operation. Cancel terminates the current process group and prevents subsequent operations. Each process has a 30-minute timeout; its remaining descendants are stopped when it finishes. Normal application exit terminates active factory processes. The next launch marks unfinished planning/execution as interrupted and requires explicit resume after inspection.

Resume verifies origins, approved local base commits, branch identities, and checkpoint SHAs. A commit interrupted between Git success and checkpoint persistence is reconciled only when its parent, reviewed tree, and factory commit message match the saved intent. Unexpected commits block execution. Dirty worktrees from interrupted implementation are retained; inspect them before acknowledging resume. The runner stages newly added files before review, captures the reviewed Git trees, and rejects changes made during review. Worktrees must be clean and match validated SHAs before publication and delivery.

Pushes, draft PR creation, companion links, and PR readiness have persisted intents. The factory checks remote branches and existing PRs before retrying. A lost creation response or partially completed readiness operation is reconciled without creating duplicate PRs. Remote branches with unexpected commits, closed PRs, and changed PR bases/SHAs block delivery. Readiness across repositories is sequential and can partially succeed; retry finishes the remaining PRs.

## PR feedback

Use **Fetch GitHub PR feedback** to retrieve general comments, inline review comments, and submitted reviews across the feature's PRs. Saved IDs, update timestamps, and content prevent reclassifying unchanged comments and detect edited review bodies. You can also paste feedback directly.

A separate read-only classifier ignores informational/unrelated comments, records in-scope requests for repair, and invalidates execution approval when scope expands. Resume in-scope repairs after inspecting the worktrees. Previously delivered PRs return to draft for repair, and the resulting validated SHAs require delivery approval again. Scope expansion requires a newly planned or edited specification revision and approval. Feedback fetching is explicit in this release, rather than an unattended background poll.

## Boundaries and roadmap

Origin credentials are stripped from repository identities before persistence or frontend delivery, and raw origin reads are not logged. The frontend never receives or persists the Worker token. Rust reads it from the OS credential store for authenticated Worker calls. The Worker remains the repository/issue integration service; factory specifications and execution state stay local. Repository activity/access is checked before execution and external-write operations. Existing issue following, claims, heartbeats, checkpoints, outbox reconciliation, and issue-comment follow-up remain available.

This release has sequential local roles and manual merging. Later milestones can add concurrent task agents and isolated task integration, reusable project playbooks and richer acceptance environments, policy-based autonomy, and optional team/cloud runners and release operations. No factory-specific Worker migration or deployment is required.

## Verification

Frontend tests cover approval payloads, recovery acknowledgement, malformed specifications, and cursor pagination. Rust tests exercise dependency ordering, immutable revisions, persisted repair budgets, restart recovery, and the shared execution boundary. A two-repository integration fixture uses real Git worktrees/local remotes with simulated Codex and GitHub responses to verify repair, acceptance commands, lost PR-creation responses, and partial readiness reconciliation. Additional fixtures cover cancellation, unavailable tools, and changed base/worktree commits.

These fixtures do not call live Codex or publish live GitHub PRs. Production end-to-end acceptance should use a disposable pair of linked repositories and the configured real CLI authentication.
