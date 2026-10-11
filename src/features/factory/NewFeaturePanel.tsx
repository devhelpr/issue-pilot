import { useState } from 'react';
import type { Issue, Repository } from '../../types';
import type { CreateFeature } from './types';

type Props = {
  repositories: Repository[];
  issues: Issue[];
  pending: boolean;
  onCreate: (input: CreateFeature) => void;
};
export function NewFeaturePanel({ repositories, issues, pending, onCreate }: Props) {
  const [title, setTitle] = useState('');
  const [brief, setBrief] = useState('');
  const [repositoryIds, setRepositoryIds] = useState<number[]>([]);
  const [sourceIssueId, setSourceIssueId] = useState<number | null>(null);
  const handleCreate = () => onCreate({ title, brief, repositoryIds, sourceIssueId });
  const handleImport = (value: string) => {
    const issue = issues.find((candidate) => candidate.id === Number(value));
    setSourceIssueId(issue?.id ?? null);
    if (issue) {
      setTitle(issue.title);
      setBrief(`${issue.body || issue.title}\n\nSource: ${issue.html_url}`);
      setRepositoryIds([issue.repository_id]);
    }
  };

  return (
    <section className="factory-panel">
      <h3>Start a feature</h3>
      <label>
        Import a GitHub issue
        <select value={sourceIssueId ?? ''} onChange={(event) => handleImport(event.target.value)}>
          <option value="">Write a new brief</option>
          {issues.map((issue) => (
            <option key={issue.id} value={issue.id}>
              {issue.full_name} #{issue.number}: {issue.title}
            </option>
          ))}
        </select>
      </label>
      <label>
        Feature title
        <input value={title} onChange={(event) => setTitle(event.target.value)} />
      </label>
      <label>
        Brief
        <textarea
          rows={7}
          value={brief}
          onChange={(event) => setBrief(event.target.value)}
          placeholder="What should change, who is it for, and what would success look like?"
        />
      </label>
      <fieldset>
        <legend>Repositories</legend>
        {repositories
          .filter(
            (repository) => Boolean(repository.active) && repository.access_status === 'active',
          )
          .map((repository) => (
            <label className="factory-check" key={repository.id}>
              <input
                type="checkbox"
                checked={repositoryIds.includes(repository.id)}
                onChange={(event) =>
                  setRepositoryIds((current) =>
                    event.target.checked
                      ? [...current, repository.id]
                      : current.filter((id) => id !== repository.id),
                  )
                }
              />
              {repository.full_name}
            </label>
          ))}
      </fieldset>
      <p>Link each selected repository to a local checkout and test command in Repositories.</p>
      <button
        disabled={pending || !title.trim() || !brief.trim() || repositoryIds.length === 0}
        onClick={handleCreate}
      >
        Create feature
      </button>
    </section>
  );
}
