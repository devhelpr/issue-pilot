# Issue Pilot Desktop

Tauri 2 desktop client for the Issue Pilot Cloudflare Worker. It stores the Worker token in the OS credential store and local settings/checkpoints/repository links in SQLite under the app-data directory.

## Development

```sh
npm install
npm run tauri dev
```

Configure the Worker URL and desktop token, discover repositories, link each active repository to a local checkout, then approve and execute an issue. The runner creates a retained worktree below `.issue-pilot-worktrees`, invokes the installed authenticated `codex exec --sandbox workspace-write`, runs the confirmed test command, and only then commits, pushes and creates a draft PR.

`codex exec` is used because the [official non-interactive mode documentation](https://developers.openai.com/docs/non-interactive-mode) describes it as the scripting interface and notes that it reuses CLI authentication. No API key is assumed.

## Architecture and limits

React renders the five screens. Rust owns all authenticated Worker requests, credentials, SQLite, process spawning, worktrees and the single-running-job mutex; the frontend never receives the token. Process arguments are passed as argument lists; only the user-confirmed test command is interpreted by a platform shell.

The Worker does not yet provide all recovery/pagination guarantees required for unattended execution. See [docs/backend-gaps.md](docs/backend-gaps.md) before production use.
