# AGENTS.md

## Project overview

Issue Pilot Desktop is a Tauri 2 desktop client:

- `src/` contains the React, TypeScript, and CSS frontend.
- `src-tauri/` contains the Rust application boundary, local SQLite storage, OS credential-store access, Worker requests, process execution, worktrees, and job orchestration.
- The frontend must never receive or persist the Worker token. Authenticated Worker calls belong in Rust.

Keep changes focused. Do not introduce a new abstraction or move files solely to satisfy this document; use the structure below for new code and refactors that already touch the area.

## Repository structure

Use a small `core` layer for code shared across features, and nested feature folders for code that primarily serves one user-facing workflow.

```text
src/
  core/                 # shared API boundary, types, constants, utilities
  components/           # reusable presentational React components
  features/
    settings/           # connection and local-tool configuration
    repositories/       # discovery, linking, and repository sync
    issues/             # issue browsing and approval
    jobs/               # job execution, status, and diagnostics
    recovery/           # checkpoints and interrupted-run recovery
  styles/               # global styles and shared design tokens
  main.tsx              # application composition and route/tab selection

src-tauri/src/
  core/                 # shared state, errors, database helpers, and Worker client
  features/
    settings/           # settings and credential operations
    repositories/       # repository links and discovery support
    issues/             # issue-related commands
    jobs/               # claims, heartbeats, execution, status, and outbox
    recovery/           # checkpoint and recovery commands
  main.rs               # Tauri setup, command registration, and composition
```

The current files may remain at their existing locations until they are naturally split. Prefer feature-local code over expanding a global utility file. Put a helper in `core` only when at least two features use it or when it defines a true application boundary.

Keep the frontend and Rust feature names aligned where practical. A feature may contain `components/`, `hooks/`, `api.ts`, `types.ts`, or Rust modules as needed; avoid empty ceremony and keep shallow nesting.

## Formatting

Prettier is the source formatter for TypeScript, TSX, and CSS. The repository configuration is in `.prettierrc.json` and uses two-space indentation, single quotes, semicolons, LF line endings, a 100-column print width, and trailing commas where supported.

- Run `npm run format` to format frontend TypeScript, TSX, and CSS files.
- Run `npm run format:check` in CI or before handoff to verify formatting without changing files.
- Keep formatting-only changes separate from behavior changes when practical.
- Do not manually fight Prettier with alignment whitespace or one-line JSX; use the configured formatter and improve readability through names and component boundaries.
- Prettier does not replace Rust formatting. Use `cargo fmt` for Rust files.

## General readability

- Prefer descriptive names that explain intent. Avoid single-letter names except for conventional short-lived loop variables.
- Keep functions and components focused on one job. Extract a named helper when a block has its own inputs, outputs, error handling, or business rule.
- Keep side effects at boundaries: React event/effect handlers, Tauri commands, database access, network calls, and process execution.
- Make state transitions visible. Use early returns for invalid state and keep the happy path easy to scan.
- Do not hide important behavior in clever one-liners, deeply nested callbacks, `any`, or broad utility modules.
- Preserve existing behavior and security boundaries when refactoring. Update nearby tests and documentation when behavior changes.
- Add comments for why a non-obvious decision exists, not for what the next line mechanically does.
- Keep code readible and not long lines

## React and TypeScript

- Use functional components and hooks. Keep `main.tsx` responsible for composition; move feature screens and substantial event logic into feature modules.
- Define explicit domain types in `core` or the owning feature. Prefer `unknown` at external boundaries, then validate or narrow it. Do not add new `any` usage.
- Keep API/request code separate from rendering. Components should call typed feature actions/hooks rather than assemble URL paths and request payloads inline.
- Name handlers with intent (`handleSave`, `handleRetry`, `handleRepositorySync`) and keep them close to the component that owns the relevant state.
- Keep effects small and cleanup-safe. Cancel async work or ignore stale results when a component can unmount or its inputs can change.
- Prefer derived values with clear names over duplicated state. Do not store values that can be calculated from existing state or props.
- Give lists stable keys from domain identifiers, not array indexes. Preserve accessible labels, button semantics, keyboard focus, and visible loading/error states.
- Keep JSX readable: use one prop per line for complex elements, extract repeated markup, and avoid deeply nested conditional expressions.
- Treat Tauri `invoke` calls as typed boundaries. Centralize command names and payload shapes where possible, and surface useful user-facing errors without leaking secrets.
- Use `npm test` for tests and `npm run build` for the TypeScript/Vite build before handing off frontend changes.

## Rust

- Keep `main.rs` focused on application setup and command registration. Put domain behavior in modules with clear ownership.
- Use small functions with explicit inputs and return `Result` for fallible operations. Propagate errors with `?`; add context at meaningful boundaries.
- Prefer domain structs and enums over loose `Value`, string flags, or positional tuples when a value crosses a module or Tauri boundary.
- Derive `Serialize`/`Deserialize` only for intentional frontend or persistence contracts. Keep internal implementation details private.
- Keep database, credential-store, HTTP, filesystem, and process concerns behind focused helpers or modules. Do not duplicate token handling or Worker URL construction.
- Use parameterized SQL for all values. Never log tokens, credentials, or unredacted sensitive command arguments.
- Avoid holding a database or async mutex guard across network calls, process execution, or other long-running work. Copy the needed values, release the guard, then perform the operation.
- Prefer argument arrays with `Command`; only invoke a shell for an explicitly user-confirmed command that genuinely requires shell interpretation.
- Make concurrency and cancellation behavior explicit. Preserve the single-running-job guarantee, checkpoints, heartbeats, and outbox semantics.
- Format Rust with `cargo fmt` and validate with `cargo check` (and targeted tests when available).

## CSS

- Keep global rules and design tokens in `src/styles/` (or the existing global stylesheet while the project is being migrated); keep feature-specific rules near the feature when that improves discoverability.
- Use class names that describe role or state (`job-card`, `diagnostic-panel`, `selected`, `offline`), not appearance or DOM position.
- Group rules by component, keep related states together, and place responsive overrides beside the relevant component rules or in a clearly marked responsive section.
- Prefer the existing design language: restrained dark surfaces, blue primary actions, readable contrast, consistent spacing, and rounded controls.
- Avoid unexplained magic numbers, excessive specificity, `!important`, and broad selectors that can unexpectedly style another feature.
- Preserve keyboard focus indicators, disabled states, readable overflow behavior, and usable layouts at narrow widths.

## Testing and handoff

Before handing off a change, run the checks relevant to the files touched:

```sh
npm test
npm run build

cd src-tauri
cargo fmt --check
cargo check
```

If a command cannot be run, say which one and why. For UI changes, verify the affected screen in `npm run tauri dev` when practical. For changes involving persistence, Worker requests, credentials, process execution, or job recovery, include the failure path in testing notes.

Keep commits and patches easy to review: separate structural moves from behavior changes when possible, avoid unrelated formatting churn, and update `README.md` or `docs/` when a workflow, limitation, or security boundary changes.
