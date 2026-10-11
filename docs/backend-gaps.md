# Worker lifecycle integration

The neighboring `../cloud-service` source now implements several guarantees previously listed here as gaps. This document describes the current integration rather than assuming the old limitations still apply.

| Capability | Current Worker implementation | Desktop behavior |
|---|---|---|
| Cursor pagination | Repository, issue, and job lists return `{ items, next_cursor }`. | Desktop follows cursors and fails visibly on malformed/repeated cursors. |
| Terminal reconciliation | Identical terminal status payloads for the saved claim reconcile after the job becomes terminal. | Existing local outbox retries the persisted terminal payload. |
| Retry safety | Retry checks issue version/state and repository activity/access. | An old or inaccessible issue snapshot cannot be retried successfully. |
| Stop/revocation enforcement | Deactivation/revocation requests stop; heartbeat and external-write status updates check current access/state. | Issue runner observes stop and validates claims before push/PR creation. |
| Claim validation | `GET /v1/jobs/:id/claim?claim_id=…` exposes current claim validity. | Issue runner checks immediately before external writes. |
| Sync progress | Worker stores queued/running/completed/failed sync state and guards issue updates by version. | Repository synchronization remains Worker-owned. |

These statements describe repository source, not the version currently deployed at a configured Worker URL. Deploy a compatible Worker before relying on these guarantees. Issue jobs still become interrupted after heartbeat expiry and require explicit recovery; the desktop does not restart them automatically.

The feature factory owns its workflow locally and does not create synthetic Worker jobs for tasks. It checks current repository activity/access through the Worker before execution and external writes. Its independent local operation ledger reconciles Git branches and GitHub PRs; see [feature-factory.md](feature-factory.md).

A claim or access check and the following GitHub write are separate operations. Revocation cannot atomically undo a push or PR action that has already begun. Multi-repository publication/readiness is also sequential; persisted intents make partial completion inspectable and retryable.
