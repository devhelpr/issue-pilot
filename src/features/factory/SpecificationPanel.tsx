import { useEffect, useState } from 'react';
import { factoryApi, isActive, parseSpecification } from './api';
import type { Feature } from './types';
import type { FactoryAction } from './useFactoryFeatures';

type PanelProps = { feature: Feature; pending: boolean; act: FactoryAction };

export function SpecificationPanel({
  feature,
  pending,
  act,
  onDirtyChange,
}: PanelProps & { onDirtyChange: (dirty: boolean) => void }) {
  const saved = feature.draft ? JSON.stringify(feature.draft.specification, null, 2) : '';
  const [editor, setEditor] = useState(saved);
  const [confirmed, setConfirmed] = useState(false);
  const active = isActive(feature);
  const dirty = editor !== saved;
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);
  if (!feature.draft) return null;
  return (
    <section className="factory-panel">
      <h3>Specification and task graph</h3>
      <p>
        Revision {feature.draft.id.slice(0, 8)} ·{' '}
        {feature.approvedRevisionId === feature.draft.id ? 'approved' : 'unapproved'}
      </p>
      {feature.draft.specification.questions.length > 0 && (
        <div className="factory-error">
          <strong>Questions to resolve</strong>
          <ul>
            {feature.draft.specification.questions.map((question) => (
              <li key={question}>{question}</li>
            ))}
          </ul>
        </div>
      )}
      <h4>Goals</h4>
      <ul>
        {feature.draft.specification.goals.map((goal) => (
          <li key={goal}>{goal}</li>
        ))}
      </ul>
      <h4>Exclusions</h4>
      <ul>
        {feature.draft.specification.exclusions.map((exclusion) => (
          <li key={exclusion}>{exclusion}</li>
        ))}
      </ul>
      <h4>Repository changes</h4>
      <ul>
        {feature.draft.specification.repositoryChanges.map((change) => (
          <li key={change}>{change}</li>
        ))}
      </ul>
      <h4>Acceptance criteria</h4>
      <ul>
        {feature.draft.specification.acceptanceCriteria.map((criterion) => (
          <li key={criterion}>{criterion}</li>
        ))}
      </ul>
      <ol className="factory-tasks">
        {feature.draft.specification.tasks.map((task) => (
          <li key={task.id}>
            <strong>{task.title}</strong>
            <span>
              {task.id} · repository {task.repositoryId} ·{' '}
              {feature.completedTasks.includes(task.id) ? 'completed' : 'pending'}
            </span>
            <p>{task.instructions}</p>
            <small>Depends on: {task.dependencies.join(', ') || 'none'}</small>
          </li>
        ))}
      </ol>
      <details>
        <summary>Edit specification</summary>
        <label>
          Specification JSON
          <textarea
            className="factory-editor"
            rows={20}
            value={editor}
            onChange={(event) => {
              setEditor(event.target.value);
              setConfirmed(false);
            }}
            disabled={active || pending}
          />
        </label>
        <button
          disabled={pending || active || !dirty}
          onClick={() =>
            void act(
              () => factoryApi.save(feature.id, parseSpecification(editor)),
              'New revision saved. Previous approval no longer authorizes execution.',
            )
          }
        >
          Save new revision
        </button>
      </details>
      <h4>Approval includes these shell commands and base commits</h4>
      {feature.draft.repositories.map((repository) => (
        <div className="factory-repository" key={repository.repositoryId}>
          <strong>
            Repository {repository.repositoryId}: {repository.baseBranch} @{' '}
            {repository.baseSha.slice(0, 12)}
          </strong>
          <code>{repository.testCommand}</code>
          <small>{repository.localPath}</small>
        </div>
      ))}
      {feature.draft.specification.checks.map((check, index) => (
        <div className="factory-repository" key={`${check.repositoryId}-${index}`}>
          <strong>Feature check · repository {check.repositoryId}</strong>
          <code>{check.command}</code>
        </div>
      ))}
      <p>
        Cross-repository commands can use <code>ISSUE_PILOT_REPO_&lt;repositoryId&gt;</code>{' '}
        environment variables to locate companion worktrees.
      </p>
      {feature.approvedRevisionId !== feature.draft.id && (
        <>
          <label className="factory-check">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
              disabled={active || dirty}
            />
            I approve this scope, base commits, shell commands, and publication of draft PRs.
          </label>
          <button
            disabled={
              pending ||
              active ||
              dirty ||
              !confirmed ||
              feature.draft.specification.questions.length > 0
            }
            onClick={() =>
              void act(
                () => factoryApi.approveSpecification(feature.id, feature.draft!.id),
                'Specification approved. Start execution when ready.',
              )
            }
          >
            Approve specification
          </button>
        </>
      )}
    </section>
  );
}
