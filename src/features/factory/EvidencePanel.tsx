import type { Feature } from './types';
const when = (value: string) => new Date(Number(value) * 1000).toLocaleString();
export function EvidencePanel({ feature }: { feature: Feature }) {
  return (
    <section className="factory-panel">
      <h3>Execution timeline and evidence</h3>
      {feature.events
        .slice(-30)
        .reverse()
        .map((entry) => (
          <p key={entry.id}>
            <time>{when(entry.createdAt)}</time> {entry.text}
          </p>
        ))}
      <details>
        <summary>Validation results ({feature.validations.length})</summary>
        {feature.validations.map((result) => (
          <article key={result.id}>
            <strong>
              {result.passed ? 'Passed' : 'Failed'} · repository {result.repositoryId} ·{' '}
              {result.taskId || 'integration'}
            </strong>
            <code>{result.command}</code>
            <pre>{result.output || '(no output)'}</pre>
          </article>
        ))}
      </details>
      <details>
        <summary>Agent and process logs ({feature.runs.length})</summary>
        {feature.runs
          .slice()
          .reverse()
          .map((run) => (
            <article key={run.id}>
              <strong>
                {run.role} · {run.status}
              </strong>
              <small>{run.logPath}</small>
              {feature.artifacts
                .filter((artifact) => artifact.runId === run.id)
                .map((artifact) => (
                  <pre key={`${artifact.runId}-${artifact.kind}`}>{artifact.text}</pre>
                ))}
            </article>
          ))}
      </details>
    </section>
  );
}
