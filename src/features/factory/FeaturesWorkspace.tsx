import { useEffect, useRef, useState } from 'react';
import type { Issue, Repository } from '../../types';
import { factoryApi } from './api';
import type { CreateFeature } from './types';
import { useFactoryFeatures } from './useFactoryFeatures';
import { NewFeaturePanel } from './NewFeaturePanel';
import { FeatureDetail } from './FeatureDetail';
import './factory.css';

type Props = { repositories: Repository[]; issues: Issue[] };
const label = (value: string) => value.replaceAll('_', ' ');
export function FeaturesWorkspace({ repositories, issues }: Props) {
  const { features, pending, error, notice, act } = useFactoryFeatures();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const feature = features.find((candidate) => candidate.id === selectedId);
  const handleCreate = (input: CreateFeature) =>
    void act(async () => {
      const created = await factoryApi.create(input);
      if (mounted.current) setSelectedId(created.id);
    }, 'Feature created. Plan it before approving execution.');
  return (
    <div className="factory-workspace">
      <div className="factory-heading">
        <div>
          <h2>Feature factory</h2>
          <p>Plan a feature, approve its scope, and deliver coordinated pull requests.</p>
        </div>
        <button onClick={() => setSelectedId(null)}>New feature</button>
      </div>
      {error && (
        <p className="factory-error" role="alert">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="factory-layout">
        <nav className="factory-inbox" aria-label="Feature inbox">
          <h3>Inbox</h3>
          {features.length === 0 && <p>No features yet. Start with a brief or import an issue.</p>}
          {features.map((item) => (
            <button
              key={item.id}
              className={selectedId === item.id ? 'selected' : ''}
              onClick={() => setSelectedId(item.id)}
            >
              <strong>{item.title}</strong>
              <span>
                {label(item.status)}
                {item.paused ? ' · paused' : ''}
              </span>
            </button>
          ))}
        </nav>
        <div className="factory-detail">
          {feature ? (
            <FeatureDetail key={feature.id} feature={feature} pending={pending} act={act} />
          ) : (
            <NewFeaturePanel
              repositories={repositories}
              issues={issues}
              pending={pending}
              onCreate={handleCreate}
            />
          )}
        </div>
      </div>
    </div>
  );
}
