import { useState } from 'react';
import { factoryApi, isActive } from './api';
import type { Feature } from './types';
import type { FactoryAction } from './useFactoryFeatures';

type PanelProps = { feature: Feature; pending: boolean; act: FactoryAction };

export function DeliveryPanel({ feature, pending, act, dirty }: PanelProps & { dirty: boolean }) {
  const [feedback, setFeedback] = useState('');
  const active = isActive(feature);
  return (
    <section className="factory-panel">
      <h3>Delivery review</h3>
      {feature.repositories.length === 0 && (
        <p>Approve a specification to prepare feature worktrees.</p>
      )}
      {feature.repositories.map((repository) => (
        <div className="factory-repository" key={repository.repositoryId}>
          <strong>
            Repository {repository.repositoryId} ·{' '}
            {repository.headSha?.slice(0, 12) || 'not committed'}
          </strong>
          <small>{repository.worktree}</small>
          {repository.prUrl && (
            <a href={repository.prUrl} target="_blank" rel="noreferrer">
              {repository.ready ? 'Ready for review' : 'Draft pull request'}
            </a>
          )}
        </div>
      ))}
      {feature.status === 'awaiting_delivery_approval' && (
        <button
          disabled={pending || dirty}
          onClick={() =>
            void act(
              () => factoryApi.approveDelivery(feature),
              'Delivery approved. PR readiness is being reconciled.',
            )
          }
        >
          Approve validated commits and mark PRs ready
        </button>
      )}
      <p>All repository and feature checks plus independent review must pass. Merging is manual.</p>
      {feature.repositories.some((repository) => repository.prUrl) && (
        <>
          <button
            disabled={pending || active || !feature.approvedRevisionId}
            onClick={() =>
              void act(
                () => factoryApi.syncFeedback(feature.id),
                'GitHub PR comments, inline comments, and reviews checked. New feedback is classified against approved scope.',
              )
            }
          >
            Fetch GitHub PR feedback
          </button>
          <label>
            Pull request feedback
            <textarea
              rows={3}
              value={feedback}
              onChange={(event) => setFeedback(event.target.value)}
              placeholder="Paste review comments or request an in-scope correction."
            />
          </label>
          <button
            disabled={pending || active || !feedback.trim()}
            onClick={() =>
              void act(
                () => factoryApi.feedback(feature.id, feedback),
                'Feedback classified. In-scope requests can be repaired; scope changes need a new specification.',
              )
            }
          >
            Review feedback
          </button>
        </>
      )}
    </section>
  );
}
