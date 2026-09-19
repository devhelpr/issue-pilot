# Worker gaps verified against `../cloud-service`

The desktop client only calls endpoints actually implemented in the Worker. These gaps block stronger guarantees and must be addressed server-side.

| Gap | Required Worker change | Acceptance criteria |
|---|---|---|
| Cursor pagination | Add `cursor` to `GET /v1/repositories`, `/issues`, and `/jobs`, returning `{ items, next_cursor }`. | A client can retrieve every record beyond 100 without duplicates while records are inserted. |
| Empty database discovery | Make `POST /v1/github/sync` discover/install GitHub App installations, or expose an authenticated installation-import endpoint. | A fresh D1 database receives repositories without a previous installation webhook. |
| Terminal reconciliation | Permit idempotent terminal `POST /v1/jobs/:id/status` for the same `{claim_id, status, commit_sha, pr_url}` and expose a reconciliation result. | A lost response after push/PR can be resent and returns the stored terminal result, including after the job is no longer `running`. |
| Retry safety | In `/retry`, verify current issue is open, repository active/access active, and the saved `issue_version` is current; return `409 issue_changed` otherwise. | An interrupted old snapshot cannot be retried after the issue changes or closes. |
| Stop/revocation enforcement | When repository access is revoked/deactivated, set `stop_requested=1` for running jobs and reject heartbeat/status external-write phases. | A runner observes stop/revocation before push and PR creation. |
| Claim validation endpoint | Add `GET /v1/jobs/:id/claim?claim_id=…` returning `{valid, stop_requested, repository_active, issue_state}`. | Desktop can check a claim immediately before push and PR creation without inferring it from an unrelated job read. |
| Sync integrity | Make webhook updates and paginated GitHub sync atomic per page/version and expose sync progress/error state. | Concurrent webhook/sync cannot regress an issue version; clients can distinguish queued, running, completed and failed sync. |

The current API has limit-only lists, `refreshRepositories` only iterates existing installation rows, and status updates select only running jobs. The desktop therefore shows these limits and leaves interrupted executions for explicit human recovery.
