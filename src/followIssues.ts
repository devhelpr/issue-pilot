import type { Issue, Repository } from './types';

export type FollowState = Record<
  string,
  {
    enabled: boolean;
    seenIssueIds: number[];
  }
>;

const STORAGE_KEY = 'issue-pilot.follow-issues.v1';

export function readFollowState(): FollowState {
  if (typeof localStorage === 'undefined') return {};
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}');
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return value as FollowState;
  } catch {
    return {};
  }
}

export function writeFollowState(state: FollowState): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Automatic following still works for the current session if storage is unavailable.
  }
}

export function observeFollowedIssues(
  state: FollowState,
  repositories: Repository[],
  issues: Issue[],
): { state: FollowState; newIssues: Issue[] } {
  const nextState: FollowState = structuredClone(state);
  const newIssues: Issue[] = [];

  for (const repository of repositories) {
    const key = String(repository.id);
    const enabled = Boolean(repository.active);
    const repositoryIssues = issues.filter(
      (issue) => issue.repository_id === repository.id && issue.state === 'open',
    );
    const issueIds = repositoryIssues.map((issue) => issue.id);
    const previous = nextState[key];

    // A first observation or a follow setting change establishes a baseline. Existing
    // issues should not unexpectedly run just because the app was opened or a repo was enabled.
    if (!previous || previous.enabled !== enabled) {
      nextState[key] = { enabled, seenIssueIds: issueIds };
      continue;
    }
    if (!enabled) continue;

    const seen = new Set(previous.seenIssueIds);
    for (const issue of repositoryIssues) {
      if (!seen.has(issue.id)) newIssues.push(issue);
    }
  }

  return { state: nextState, newIssues };
}

export function markIssueSeen(state: FollowState, issue: Issue): FollowState {
  const key = String(issue.repository_id);
  const previous = state[key] || { enabled: true, seenIssueIds: [] };
  if (previous.seenIssueIds.includes(issue.id)) return state;
  return {
    ...state,
    [key]: {
      ...previous,
      seenIssueIds: [...previous.seenIssueIds, issue.id],
    },
  };
}
