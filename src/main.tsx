import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import ReactMarkdown from 'react-markdown';
import { createApi } from './api';
import {
  markIssueSeen,
  observeFollowedIssues,
  readFollowState,
  writeFollowState,
} from './followIssues';
import type { Issue, Job, JobDebugEvent, Repository, Settings } from './types';
import './style.css';

const id = () => crypto.randomUUID();
const api = createApi((path, init = {}) =>
  invoke('worker_request', {
    method: init.method ?? 'GET',
    path,
    body: init.body ? JSON.parse(init.body as string) : null,
    idempotencyKey: (init.headers as Record<string, string> | undefined)?.['Idempotency-Key'],
  }),
);
type Tab = 'settings' | 'repositories' | 'issues' | 'jobs' | 'recovery';
function App() {
  const [tab, setTab] = useState<Tab>('settings'),
    [settings, setSettings] = useState<Settings | null>(null),
    [repos, setRepos] = useState<Repository[]>([]),
    [issues, setIssues] = useState<Issue[]>([]),
    [jobs, setJobs] = useState<Job[]>([]),
    [selected, setSelected] = useState<Issue | null>(null),
    [message, setMessage] = useState('Configure the Worker connection to begin.'),
    [offline, setOffline] = useState(false),
    [filter, setFilter] = useState<number | undefined>(),
    [showFollowingOnly, setShowFollowingOnly] = useState(false),
    [recovery, setRecovery] = useState<any[]>([]),
    [repositoryDebug, setRepositoryDebug] = useState<unknown>(null),
    [syncDebug, setSyncDebug] = useState<unknown>(null),
    [jobDebug, setJobDebug] = useState<JobDebugEvent[]>([]),
    [jobServerSnapshot, setJobServerSnapshot] = useState<unknown>(null),
    [jobRunResult, setJobRunResult] = useState<unknown>(null),
    [autoQueue, setAutoQueue] = useState<Issue[]>([]),
    [activeJobId, setActiveJobId] = useState<string | null>(null),
    [autoStartedJobIds, setAutoStartedJobIds] = useState<Set<string>>(() => new Set());
  const busy = useRef(false);
  const delay = useRef(10_000);
  const followState = useRef(readFollowState());
  const autoRunning = useRef(false);
  const runningJobId = useRef<string | null>(null);
  const load = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    const endpoint = '/v1/repositories?limit=100';
    try {
      const r = await api.repositories();
      setRepos(r.items);
      setRepositoryDebug({
        at: new Date().toISOString(),
        request: { method: 'GET', endpoint },
        ok: true,
        status: 'response received',
        raw_response: r,
        count: r.items.length,
        items: r.items.map((repo) => ({
          id: repo.id,
          full_name: repo.full_name,
          active: Boolean(repo.active),
          access_status: repo.access_status,
        })),
      });
      try {
        const [i, j] = await Promise.all([api.issues(), api.jobs()]);
        const observed = observeFollowedIssues(followState.current, r.items, i.items);
        followState.current = observed.state;
        writeFollowState(followState.current);
        setAutoQueue((current) => {
          const queued = new Set(current.map((issue) => issue.id));
          return [...current, ...observed.newIssues.filter((issue) => !queued.has(issue.id))];
        });
        setIssues(filter ? i.items.filter((issue) => issue.repository_id === filter) : i.items);
        setJobs(j.items);
        setOffline(false);
        delay.current = 10_000;
      } catch (e: any) {
        setOffline(false);
        setMessage(`Repositories loaded, but another Worker request failed: ${e.message || e}`);
        delay.current = Math.min(delay.current * 2, 60_000);
      }
    } catch (e: any) {
      setRepositoryDebug({
        at: new Date().toISOString(),
        request: { method: 'GET', endpoint },
        ok: false,
        error: e.message || String(e),
      });
      setOffline(true);
      setMessage(e.message || 'Unable to read repositories from Worker');
      delay.current = Math.min(delay.current * 2, 60_000);
    } finally {
      busy.current = false;
    }
  }, [filter]);
  useEffect(() => {
    invoke<Settings>('get_settings')
      .then(setSettings)
      .catch((e) => setMessage(String(e)));
    invoke<any[]>('list_checkpoints').then(setRecovery);
  }, []);
  useEffect(() => {
    if (!settings?.hasToken) return;
    let cancelled = false;
    const tick = async () => {
      await load();
      if (!cancelled) setTimeout(tick, delay.current);
    };
    tick();
    return () => {
      cancelled = true;
    };
  }, [settings?.hasToken, load]);
  const approve = async (issue: Issue) => {
    try {
      setMessage('Approving issue…');
      await api.approve(issue.id, issue.version, id());
      setMessage('Issue approved and queued.');
      await load();
    } catch (e: any) {
      setMessage(e.message);
    }
  };
  const syncIssues = useCallback(async (repo: Repository) => {
    const endpoint = `/v1/repositories/${repo.id}/sync`;
    try {
      const result = await api.syncRepository(repo.id);
      setSyncDebug({
        at: new Date().toISOString(),
        repository: { id: repo.id, full_name: repo.full_name },
        request: { method: 'POST', endpoint },
        ok: true,
        status: 'response received',
        raw_response: result,
      });
      setMessage('Issue sync requested.');
    } catch (e: any) {
      setSyncDebug({
        at: new Date().toISOString(),
        repository: { id: repo.id, full_name: repo.full_name },
        request: { method: 'POST', endpoint },
        ok: false,
        error: e.message || String(e),
      });
      setMessage(e.message || 'Could not request issue sync.');
    }
  }, []);
  const inspectJob = async (jobId?: string) => {
    if (!jobId) return;
    const [local, server] = await Promise.allSettled([
      invoke<JobDebugEvent[]>('list_job_debug', { jobId }),
      api.job(jobId),
    ]);
    if (local.status === 'fulfilled') setJobDebug(local.value);
    else
      setMessage(`Could not read local job diagnostics: ${local.reason?.message || local.reason}`);
    if (server.status === 'fulfilled') setJobServerSnapshot(server.value);
    else setJobServerSnapshot({ error: server.reason?.message || String(server.reason), jobId });
  };
  useEffect(() => {
    if (!activeJobId) return;
    let cancelled = false;
    const refreshActiveJob = async () => {
      const [local, remote] = await Promise.allSettled([
        invoke<JobDebugEvent[]>('list_job_debug', { jobId: activeJobId }),
        api.job(activeJobId),
      ]);
      if (cancelled) return;
      if (local.status === 'fulfilled') setJobDebug(local.value);
      if (remote.status === 'fulfilled') {
        setJobs((current) => current.map((job) => (job.id === activeJobId ? remote.value : job)));
      }
    };
    void refreshActiveJob();
    const interval = window.setInterval(refreshActiveJob, 1_500);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activeJobId]);
  const run = async (job: Job, issueOverride?: Issue, startedAutomatically = false) => {
    if (runningJobId.current) {
      setMessage(`Job ${runningJobId.current} is already running.`);
      return;
    }
    const issue = issueOverride || issues.find((i) => i.id === job.issue_id);
    if (!issue) {
      setMessage(
        'Cannot start: the issue is not in the loaded open-issue list. Refresh before running.',
      );
      setJobRunResult({ jobId: job.id, ok: false, error: 'Missing issue/repository mapping' });
      await inspectJob(job.id);
      return;
    }
    const enriched = {
      ...job,
      repository_id: issue.repository_id,
      issue_number: issue.number,
      issue_url: issue.html_url,
    };
    runningJobId.current = job.id;
    try {
      setActiveJobId(job.id);
      setJobDebug([]);
      setJobRunResult(null);
      if (startedAutomatically) {
        setAutoStartedJobIds((current) => new Set(current).add(job.id));
      }
      setMessage('Claiming and starting local runner…');
      const result = await invoke('execute_job', { job: enriched });
      setJobRunResult({ jobId: job.id, ok: true, result });
      setMessage('Local runner finished; Worker status is being refreshed.');
      await load();
      setRecovery(await invoke('list_checkpoints'));
      await inspectJob(job.id);
    } catch (e: any) {
      const errorMessage = e.message || String(e);
      if (errorMessage.includes('Another job is already executing')) {
        setMessage('Another job is already running. This start request was ignored.');
        setActiveJobId(null);
        await load();
      } else {
        setJobRunResult({ jobId: job.id, ok: false, error: errorMessage });
        setMessage(`Job stopped: ${errorMessage}`);
        await inspectJob(job.id);
      }
    } finally {
      if (runningJobId.current === job.id) {
        runningJobId.current = null;
        setActiveJobId(null);
      }
    }
  };
  useEffect(() => {
    if (autoRunning.current || !autoQueue.length) return;
    const issue = autoQueue[0];
    const repository = repos.find((repo) => repo.id === issue.repository_id);
    if (!repository || !Boolean(repository.active)) {
      setAutoQueue((current) => current.slice(1));
      return;
    }

    autoRunning.current = true;
    void (async () => {
      try {
        setMessage(`New issue detected in ${issue.full_name}; creating a job…`);
        let approvalError: unknown;
        try {
          await api.approve(issue.id, issue.version, `auto-follow-${issue.id}-${issue.version}`);
        } catch (error) {
          approvalError = error;
        }

        const refreshedJobs = await api.jobs();
        setJobs(refreshedJobs.items);
        const job = refreshedJobs.items.find((candidate) => candidate.issue_id === issue.id);
        if (!job) {
          throw approvalError || new Error('Worker did not return a job for the new issue.');
        }

        followState.current = markIssueSeen(followState.current, issue);
        writeFollowState(followState.current);
        if (job.status === 'queued') {
          await run(job, issue, true);
        } else {
          setMessage(`Job for issue #${issue.number} is already ${job.status}.`);
        }
      } catch (error: any) {
        setMessage(
          `Could not automatically start issue #${issue.number}: ${error.message || error}`,
        );
      } finally {
        autoRunning.current = false;
        setAutoQueue((current) => current.slice(1));
      }
    })();
  }, [autoQueue, repos, issues]);
  const content = useMemo(() => {
    if (tab === 'settings')
      return (
        <SettingsPane
          settings={settings}
          onSaved={async () => setSettings(await invoke('get_settings'))}
          message={message}
        />
      );
    if (tab === 'repositories') {
      const visibleRepos = showFollowingOnly ? repos.filter((r) => Boolean(r.active)) : repos;
      return (
        <>
          <section className="actions">
            <button
              onClick={async () => {
                try {
                  const result = (await api.syncGithub()) as { repositories: number };
                  setRepositoryDebug({
                    at: new Date().toISOString(),
                    request: { method: 'POST', endpoint: '/v1/github/sync' },
                    ok: true,
                    result,
                  });
                  await load();
                  setMessage(
                    result.repositories
                      ? `Discovered ${result.repositories} GitHub ${result.repositories === 1 ? 'repository' : 'repositories'}.`
                      : 'No repositories found. Confirm the GitHub App is installed for an allowed account and has repository access.',
                  );
                } catch (e: any) {
                  setRepositoryDebug({
                    at: new Date().toISOString(),
                    request: { method: 'POST', endpoint: '/v1/github/sync' },
                    ok: false,
                    error: e.message || String(e),
                  });
                  setMessage(e.message || 'Repository discovery failed.');
                }
              }}
            >
              Discover GitHub repositories
            </button>
            <button onClick={load}>Refresh</button>
            <label className="check-control filter-control">
              <input
                type="checkbox"
                checked={showFollowingOnly}
                onChange={(e) => setShowFollowingOnly(e.target.checked)}
              />
              <span className="check-box" aria-hidden="true" />
              Only show repositories where Follow issues is enabled
            </label>
          </section>
          {repositoryDebug && (
            <details>
              <summary>Repository request diagnostic</summary>
              <DiagnosticBlock value={repositoryDebug} />
            </details>
          )}
          {syncDebug && (
            <details>
              <summary>Issue sync diagnostic</summary>
              <DiagnosticBlock value={syncDebug} />
            </details>
          )}
          {!repos.length && <p className="hint">No repositories have been discovered yet.</p>}
          {repos.length > 0 && !visibleRepos.length && (
            <p className="hint">No repositories are currently set to follow issues.</p>
          )}
          <div className="cards">
            {visibleRepos.map((r) => (
              <article className="card" key={r.id}>
                <h3>{r.full_name}</h3>
                <small className={`repo-access ${r.access_status}`}>
                  {r.access_status === 'active'
                    ? 'GitHub access available'
                    : 'GitHub access revoked'}
                </small>
                <label className="check-control">
                  <input
                    type="checkbox"
                    checked={Boolean(r.active)}
                    disabled={r.access_status !== 'active'}
                    onChange={async (e) => {
                      try {
                        await api.setRepository(r.id, e.target.checked);
                        await load();
                      } catch (error: any) {
                        setMessage(error.message || 'Could not update repository.');
                      }
                    }}
                  />
                  <span className="check-box" aria-hidden="true" />
                  Follow issues
                </label>
                <RepoLink repo={r} onSaved={() => setMessage('Local checkout saved.')} />
                <button onClick={() => syncIssues(r)}>Sync issues</button>
              </article>
            ))}
          </div>
        </>
      );
    }
    if (tab === 'issues')
      return (
        <>
          <div className="actions">
            <select
              value={filter ?? ''}
              onChange={(e) => setFilter(e.target.value ? Number(e.target.value) : undefined)}
            >
              <option value="">All repositories</option>
              {repos.map((r) => (
                <option value={r.id} key={r.id}>
                  {r.full_name}
                </option>
              ))}
            </select>
            <button onClick={load}>Refresh</button>
          </div>
          <div className="split">
            <div className="list">
              {issues.map((i) => (
                <button
                  className={selected?.id === i.id ? 'selected' : ''}
                  onClick={() => setSelected(i)}
                  key={i.id}
                >
                  <b>
                    #{i.number} {i.title}
                  </b>
                  <small>
                    {i.full_name} · {i.github_updated_at}
                  </small>
                </button>
              )) || <p>No issues.</p>}
            </div>
            <article className="detail">
              {selected ? (
                <>
                  {(() => {
                    const issueJob = jobs.find((job) => job.issue_id === selected.id);
                    if (!issueJob) return null;
                    return (
                      <section className="issue-job-status" aria-live="polite">
                        <h3>
                          {autoStartedJobIds.has(issueJob.id)
                            ? 'Started automatically'
                            : issueJob.status === 'queued'
                              ? 'Approved and queued'
                              : 'Job status'}
                        </h3>
                        <JobProgress
                          job={issueJob}
                          events={issueJob.id === activeJobId ? jobDebug : []}
                          locallyRunning={issueJob.id === activeJobId}
                        />
                        <button className="secondary" onClick={() => setTab('jobs')}>
                          View job details
                        </button>
                      </section>
                    );
                  })()}
                  <h2>
                    #{selected.number} {selected.title}
                  </h2>
                  <p>
                    {labels(selected.labels_json).map((x) => (
                      <span className="label" key={x}>
                        {x}
                      </span>
                    ))}
                  </p>
                  <ReactMarkdown skipHtml>{selected.body || '_No description._'}</ReactMarkdown>
                  {!jobs.some((job) => job.issue_id === selected.id) && (
                    <button
                      onClick={() => approve(selected)}
                      disabled={!Boolean(selected.active) || selected.state !== 'open'}
                    >
                      Approve for local execution
                    </button>
                  )}
                </>
              ) : (
                <p>Select an issue to review its immutable version before approval.</p>
              )}
            </article>
          </div>
        </>
      );
    if (tab === 'jobs')
      return (
        <div className="jobs-page">
          <p className="hint">
            The app executes at most one job. When Follow issues is enabled, issues discovered after
            the current baseline are approved, queued and started automatically. Jobs require a
            linked checkout.
          </p>
          {Boolean(jobRunResult) && <RunnerResult result={jobRunResult} />}
          <div className="job-list">
            {jobs.map((j) => {
              const issue = issues.find((i) => i.id === j.issue_id);
              return (
                <article className="card job-card" key={j.id}>
                  <h3>{j.issue_title}</h3>
                  <p className="job-status">
                    <b>{j.status}</b> {j.phase && `· ${j.phase}`}
                    {j.stop_requested && ' · stop requested'}
                  </p>
                  {(activeJobId === j.id ||
                    ['running', 'claimed', 'in_progress'].includes(j.status)) && (
                    <JobProgress
                      job={j}
                      events={j.id === activeJobId ? jobDebug : []}
                      locallyRunning={j.id === activeJobId}
                    />
                  )}
                  {j.result_summary && (
                    <p className="job-result-summary">{compactSummary(j.result_summary)}</p>
                  )}
                  {j.pr_url && (
                    <p>
                      <a href={j.pr_url}>Open draft PR</a>
                    </p>
                  )}
                  <div className="job-actions">
                    {j.status === 'queued' && (
                      <>
                        <button onClick={() => run(j)} disabled={!issue || activeJobId !== null}>
                          Run locally
                        </button>
                        {!issue && (
                          <p className="hint">Issue details are not loaded; refresh first.</p>
                        )}
                      </>
                    )}
                    {['interrupted', 'failed', 'cancelled'].includes(j.status) && (
                      <button
                        onClick={async () => {
                          try {
                            await api.retry(j.id);
                            await load();
                          } catch (e: any) {
                            setMessage(e.message || String(e));
                          }
                        }}
                      >
                        Create retry after inspection
                      </button>
                    )}
                    <button className="secondary" onClick={() => inspectJob(j.id)}>
                      Show local diagnostics
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
          {Boolean(jobServerSnapshot) && (
            <details className="diagnostic-panel" open>
              <summary>Worker job snapshot</summary>
              <DiagnosticBlock value={jobServerSnapshot} />
            </details>
          )}
          {jobDebug.length > 0 && (
            <details className="diagnostic-panel" open>
              <summary>Local runner diagnostics ({jobDebug.length} events)</summary>
              <div className="debug-list">
                {jobDebug.map((event) => (
                  <article className={`debug-event ${event.level}`} key={event.id}>
                    <p>
                      <b>{event.level}</b> · {event.phase || 'general'} · {event.message}{' '}
                      <small>{event.created_at}</small>
                    </p>
                    {event.detail && <DiagnosticBlock value={event.detail} />}
                  </article>
                ))}
              </div>
            </details>
          )}
        </div>
      );
    return (
      <>
        <p className="hint">
          Interrupted jobs are never resumed automatically. Inspect the retained worktree, branch,
          SHA and server state before explicitly retrying.
        </p>
        <div className="cards">
          {recovery.map((c) => (
            <article className="card" key={c.job_id}>
              <h3>{c.job_id}</h3>
              <p>
                {c.phase} · {c.updated_at}
              </p>
              <code>{c.worktree}</code>
              {c.commit_sha && <p>Commit: {c.commit_sha}</p>}
              {c.pr_url && <a href={c.pr_url}>Open PR</a>}
            </article>
          ))}
          {!recovery.length && <p>No local checkpoints yet.</p>}
        </div>
      </>
    );
  }, [
    tab,
    settings,
    message,
    repos,
    issues,
    selected,
    jobs,
    recovery,
    filter,
    showFollowingOnly,
    load,
    repositoryDebug,
    syncDebug,
    syncIssues,
    jobDebug,
    jobServerSnapshot,
    jobRunResult,
    activeJobId,
    autoStartedJobIds,
    run,
  ]);
  return (
    <main>
      <aside>
        <h1>Issue Pilot</h1>
        {(['settings', 'repositories', 'issues', 'jobs', 'recovery'] as Tab[]).map((t) => (
          <button key={t} className={tab === t ? 'nav active' : 'nav'} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
        <footer className={offline ? 'offline' : ''}>
          {offline ? 'Offline · retrying' : 'Connected / ready'}
        </footer>
      </aside>
      <section className="workspace">
        <header>{message}</header>
        {content}
      </section>
    </main>
  );
}
function formatDiagnostic(value: unknown): string {
  const parsed =
    typeof value === 'string'
      ? (() => {
          try {
            return JSON.parse(value);
          } catch {
            return value;
          }
        })()
      : value;
  const text =
    typeof parsed === 'string' ? parsed : (JSON.stringify(parsed, null, 2) ?? String(parsed));
  return text
    .replace(/\\r\\n/g, '\n')
    .replace(/\\n/g, '\n')
    .replace(/\\t/g, '\t');
}
function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
function progressFor(job: Job, events: JobDebugEvent[]) {
  const terminal = ['succeeded', 'completed', 'failed', 'cancelled', 'interrupted'].includes(
    job.status,
  );
  const phase = (events[0]?.phase || job.phase || '').toLowerCase();
  const stages: Record<string, { label: string; percent: number }> = {
    preflight: { label: 'Starting local runner', percent: 8 },
    claim: { label: 'Claiming job', percent: 14 },
    preparing: { label: 'Preparing isolated checkout', percent: 22 },
    analyzing: { label: 'Starting work', percent: 22 },
    fixing: { label: 'Working on the issue', percent: 42 },
    testing: { label: 'Running tests', percent: 62 },
    committing: { label: 'Committing changes', percent: 79 },
    creating_pr: { label: 'Creating draft pull request', percent: 92 },
    completed: { label: 'Job completed', percent: 100 },
    failed: { label: 'Job stopped', percent: 100 },
  };
  const stage = stages[phase] || { label: 'Starting local runner', percent: 8 };
  const activity = events.find(
    (event) =>
      !['Worker request started', 'Worker request succeeded', 'Local command finished'].includes(
        event.message,
      ),
  )?.message;
  return {
    ...stage,
    terminal,
    activity,
  };
}
function JobProgress({
  job,
  events,
  locallyRunning,
}: {
  job: Job;
  events: JobDebugEvent[];
  locallyRunning: boolean;
}) {
  if (job.status === 'queued' && !locallyRunning) {
    return <p className="job-queue-note">Waiting in the queue to run locally.</p>;
  }
  const progress = progressFor(job, events);
  const active =
    !progress.terminal &&
    (locallyRunning || ['running', 'claimed', 'in_progress'].includes(job.status));
  return (
    <div className="job-progress" aria-live="polite">
      <div className="job-progress-heading">
        {active ? (
          <span className="job-spinner" aria-hidden="true" />
        ) : (
          <span className="job-progress-dot" />
        )}
        <b>{progress.label}</b>
        {active && <span className="job-progress-percent">{progress.percent}%</span>}
      </div>
      <div
        className="job-progress-track"
        role="progressbar"
        aria-label="Job progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress.percent}
      >
        <span style={{ width: `${progress.percent}%` }} />
      </div>
      <p>{active ? progress.activity || 'The local agent is working…' : job.status}</p>
    </div>
  );
}
function compactSummary(summary: string): string {
  const trimmed = summary.trim();
  if (!trimmed) return '';
  try {
    const parsed: unknown = JSON.parse(trimmed);
    const value = asRecord(parsed);
    const short = [value.summary, value.message, value.result].find(
      (candidate) => typeof candidate === 'string' && candidate.trim(),
    ) as string | undefined;
    if (short) return short.trim().slice(0, 220);
    if (typeof value.status === 'string') return `Job ${value.status}.`;
  } catch {
    // Worker summaries are often plain text rather than JSON.
  }
  const singleLine = trimmed.replace(/\s+/g, ' ');
  return singleLine.length > 220 ? `${singleLine.slice(0, 217)}…` : singleLine;
}
function RunnerResult({ result }: { result: unknown }) {
  const envelope = asRecord(result);
  const data = asRecord(envelope.result);
  const ok = envelope.ok === true;
  const status = typeof data.status === 'string' ? data.status : ok ? 'finished' : 'stopped';
  const error = typeof envelope.error === 'string' ? envelope.error : '';
  const sha = typeof data.commit_sha === 'string' ? data.commit_sha : '';
  const prUrl = typeof data.pr_url === 'string' ? data.pr_url : '';
  const shortSha = sha ? sha.slice(0, 8) : '';
  return (
    <section className={`runner-result ${ok ? 'success' : 'failure'}`} aria-live="polite">
      <div className="runner-result-heading">
        <h3>Last run: {status}</h3>
        {ok && (
          <span className="result-check" aria-label="Successful">
            ✓
          </span>
        )}
      </div>
      {error && <p>{compactSummary(error)}</p>}
      {shortSha && (
        <p>
          Commit <code>{shortSha}</code>
        </p>
      )}
      {prUrl && <a href={prUrl}>Open draft pull request</a>}
      {data.worker_reported === false && (
        <p>Result saved locally; Worker status is still pending.</p>
      )}
      <details>
        <summary>Full runner details</summary>
        <DiagnosticBlock value={result} />
      </details>
    </section>
  );
}
function DiagnosticBlock({ value, className = '' }: { value: unknown; className?: string }) {
  return <pre className={`diagnostic ${className}`.trim()}>{formatDiagnostic(value)}</pre>;
}
function labels(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}
function SettingsPane({
  settings,
  onSaved,
  message,
}: {
  settings: Settings | null;
  onSaved: () => Promise<void>;
  message: string;
}) {
  const [url, setUrl] = useState(''),
    [token, setToken] = useState(''),
    [codex, setCodex] = useState('codex'),
    [git, setGit] = useState('git'),
    [gh, setGh] = useState('gh'),
    [tools, setTools] = useState<any>(),
    [connection, setConnection] = useState<any>();
  useEffect(() => {
    if (settings) {
      setUrl(settings.workerUrl);
      setCodex(settings.codexPath);
      setGit(settings.gitPath);
      setGh(settings.ghPath);
    }
  }, [settings]);
  return (
    <section className="form">
      <h2>Connection and local tools</h2>
      <p>The Worker token is held by the OS credential store, never browser storage.</p>
      <label>
        Worker URL
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://issue-pilot.example.workers.dev"
        />
      </label>
      <label>
        Desktop token
        <input
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder={
            settings?.hasToken ? 'Stored securely (enter only to replace)' : 'Paste token'
          }
        />
      </label>
      <div className="grid">
        <label>
          Codex path
          <input value={codex} onChange={(e) => setCodex(e.target.value)} />
        </label>
        <label>
          Git path
          <input value={git} onChange={(e) => setGit(e.target.value)} />
        </label>
        <label>
          gh path
          <input value={gh} onChange={(e) => setGh(e.target.value)} />
        </label>
      </div>
      <button
        onClick={async () => {
          await invoke('save_settings', {
            input: {
              workerUrl: url,
              token: token || null,
              codexPath: codex,
              gitPath: git,
              ghPath: gh,
            },
          });
          setToken('');
          await onSaved();
          setConnection(await invoke('check_worker_connection'));
        }}
      >
        Save securely
      </button>
      <button onClick={async () => setConnection(await invoke('check_worker_connection'))}>
        Test Worker connection
      </button>
      <button onClick={async () => setTools(await invoke('check_tools'))}>Check CLI access</button>
      {connection && (
        <>
          <h3>Worker connection result</h3>
          <DiagnosticBlock value={connection} className={connection.ok ? 'success' : 'error'} />
        </>
      )}
      {tools && <DiagnosticBlock value={tools} />}
      <p className="hint">{message}</p>
    </section>
  );
}
function RepoLink({ repo, onSaved }: { repo: Repository; onSaved: () => void }) {
  const [path, setPath] = useState(''),
    [base, setBase] = useState('main'),
    [tests, setTests] = useState('npm test'),
    [loaded, setLoaded] = useState(false),
    [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    setLoaded(false);
    setError('');
    invoke<{
      repositoryId: number;
      localPath: string;
      baseBranch: string;
      testCommand: string;
    } | null>('get_repo_link', { repositoryId: repo.id })
      .then((link) => {
        if (cancelled) return;
        if (link) {
          setPath(link.localPath);
          setBase(link.baseBranch);
          setTests(link.testCommand);
        }
        setLoaded(true);
      })
      .catch((e) => {
        if (!cancelled) {
          setLoaded(true);
          setError(e.message || String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [repo.id]);
  return (
    <details>
      <summary>{loaded && path ? `Linked checkout: ${path}` : 'Link local checkout'}</summary>
      {error && <p className="error-text">Could not load saved checkout settings: {error}</p>}
      <label>
        Path
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/Users/me/code/repo"
        />
      </label>
      <label>
        Base branch
        <input value={base} onChange={(e) => setBase(e.target.value)} />
      </label>
      <label>
        Confirmed test command
        <input value={tests} onChange={(e) => setTests(e.target.value)} />
      </label>
      <button
        disabled={!loaded}
        onClick={async () => {
          try {
            await invoke('save_repo_link', {
              input: {
                repositoryId: repo.id,
                localPath: path,
                baseBranch: base,
                testCommand: tests,
              },
            });
            onSaved();
          } catch (e: any) {
            setError(e.message || String(e));
          }
        }}
      >
        {loaded ? 'Save link' : 'Loading saved settings…'}
      </button>
    </details>
  );
}
export default App;

const root = document.getElementById('root');
if (!root) throw new Error('Issue Pilot root element is missing');
createRoot(root).render(<App />);
