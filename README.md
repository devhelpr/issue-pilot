# Issue Pilot Desktop and Feature Factory

Tauri 2 desktop client for the Issue Pilot Cloudflare Worker. It stores the Worker token in the OS credential store and local settings/checkpoints/repository links in SQLite under the app-data directory.

## Development

```sh
npm install
npm run tauri dev
```

Configure the Worker URL and desktop token, discover repositories, and link each active repository to a local checkout. When `Follow issues` is enabled, the desktop establishes a baseline and automatically approves, creates and starts a job for issues discovered afterward. The runner creates a retained worktree below `.issue-pilot-worktrees`, invokes the installed authenticated `codex exec --sandbox workspace-write`, runs the confirmed test command, and only then commits, pushes and creates a draft PR.

For successful jobs with an open pull request, the desktop periodically checks issue comments received by the Worker’s GitHub webhook. Codex classifies new comments in read-only mode. Comments and unrelated feedback are ignored; change requests are applied in the original retained worktree, tested, committed, and pushed to the existing PR branch. Reviewed comment IDs and decisions are stored in the local SQLite database. On the first scan, comments last updated before job creation are treated as the initial baseline. If the Worker does not include a job creation time, comments already present at the first scan become the baseline.

`codex exec` is used because the [official non-interactive mode documentation](https://developers.openai.com/docs/non-interactive-mode) describes it as the scripting interface and notes that it reuses CLI authentication. No API key is assumed.

## Feature factory

Open **Features** to turn a brief or imported GitHub issue into an approved specification, dependency-ordered tasks, validated changes across repositories, and linked draft PRs. Planner and reviewer runs are read-only; implementation, automatic repair, Git operations, and recovery are orchestrated in Rust. A second approval binds delivery to validated commits and marks the PRs ready for review. Merging stays manual.

Factory state stays in local SQLite alongside existing issue-runner data. Both workflows share one exclusive runner. See [the feature factory guide](docs/feature-factory.md) for setup, approvals, cross-repository checks, feedback, and recovery.

## Architecture and limits

React renders the Features workspace and existing issue-runner screens. Rust owns all authenticated Worker requests, credentials, SQLite, process spawning, worktrees and the single-running-job mutex; the frontend never receives the token. Process arguments are passed as argument lists; only the user-confirmed test command is interpreted by a platform shell.

The current Worker source provides cursor pagination, claim validation, retry guards, and terminal reconciliation. See [docs/backend-gaps.md](docs/backend-gaps.md) for integration requirements and remaining recovery boundaries.

The Jobs screen's “Show local diagnostics” action displays the local runner trace alongside the current Worker job snapshot. It includes claim/status request paths, request IDs, phase changes, heartbeat failures, CLI exit codes, retained worktree paths, and terminal-status outbox state. A successful local run commits, pushes, creates a draft PR, and reports the commit and PR URL back to the Worker; if that final response is lost, the terminal payload is retried from the local outbox on the next run.
