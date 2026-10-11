import { useEffect, useState } from 'react';
import { factoryApi, isActive } from './api';
import type { Feature } from './types';
import type { FactoryAction } from './useFactoryFeatures';

type PanelProps = { feature: Feature; pending: boolean; act: FactoryAction };
import { SpecificationPanel } from './SpecificationPanel';
import { DeliveryPanel } from './DeliveryPanel';
import { EvidencePanel } from './EvidencePanel';
const label = (value: string) => value.replaceAll('_', ' ');

export function FeatureDetail({ feature, pending, act }: PanelProps) {
  const [message, setMessage] = useState('');
  const [inspected, setInspected] = useState(false);
  const [dirty, setDirty] = useState(false);
  const active = isActive(feature);
  const revisionId = feature.draft?.id;
  useEffect(() => {
    setInspected(false);
    setMessage('');
  }, [revisionId]);
  return (
    <>
      <section className="factory-panel">
        <h3>{feature.title}</h3>
        <p className="factory-status" role="status">
          {label(feature.status)}
          {feature.paused ? ' · paused' : ''}
        </p>
        <p className="factory-brief">{feature.brief}</p>
        {feature.error && (
          <p className="factory-error" role="alert">
            {feature.error}
          </p>
        )}
        <div className="factory-actions">
          {feature.approvedRevisionId &&
            !active &&
            feature.status !== 'delivered' &&
            feature.status !== 'cancelled' &&
            feature.status !== 'awaiting_specification_approval' &&
            feature.status !== 'awaiting_delivery_approval' && (
              <button
                disabled={pending || dirty || (!!feature.error && !inspected)}
                onClick={() =>
                  void act(
                    () => factoryApi.execute(feature.id, inspected),
                    'Execution started. Progress is saved locally.',
                  )
                }
              >
                {feature.error ? 'Resume and validate' : 'Execute approved feature'}
              </button>
            )}
          {active && (
            <button
              onClick={() =>
                void act(
                  () => factoryApi.control(feature.id, 'pause'),
                  'Pause requested; the current process will finish first.',
                )
              }
            >
              Pause
            </button>
          )}
          {(active || (feature.status !== 'cancelled' && feature.status !== 'delivered')) && (
            <button
              onClick={() =>
                void act(
                  () => factoryApi.control(feature.id, 'cancel'),
                  'Cancellation requested. Worktrees are retained.',
                )
              }
            >
              Cancel
            </button>
          )}
        </div>
        {feature.error && feature.approvedRevisionId && !active && (
          <label className="factory-check">
            <input
              type="checkbox"
              checked={inspected}
              onChange={(event) => setInspected(event.target.checked)}
            />
            I inspected the retained worktrees and failure evidence before resuming.
          </label>
        )}
      </section>
      <section className="factory-panel">
        <h3>Planning conversation</h3>
        <div className="factory-conversation">
          {feature.messages.map((entry) => (
            <article key={entry.id}>
              <strong>{entry.role}</strong>
              <p>{entry.text}</p>
            </article>
          ))}
        </div>
        <label>
          Your clarification or planning request
          <textarea
            rows={3}
            value={message}
            onChange={(event) => setMessage(event.target.value)}
            placeholder="Explain requirements, answer questions, or request a revised plan."
          />
        </label>
        <button
          disabled={pending || active}
          onClick={() =>
            void act(
              () =>
                factoryApi.plan(
                  feature.id,
                  message ||
                    'Inspect the selected repositories and propose a feature specification.',
                ),
              'Planning result saved. Review the feature details.',
            )
          }
        >
          Run read-only planner
        </button>
        {pending && (
          <p role="status">Working… You can leave this screen; results are saved locally.</p>
        )}
      </section>
      {feature.draft && (
        <SpecificationPanel
          key={feature.draft.id}
          feature={feature}
          pending={pending}
          act={act}
          onDirtyChange={setDirty}
        />
      )}
      <DeliveryPanel feature={feature} pending={pending} act={act} dirty={dirty} />
      <EvidencePanel feature={feature} />
    </>
  );
}
