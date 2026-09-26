import { describe, expect, it } from 'vitest';
import { markIssueSeen, observeFollowedIssues, type FollowState } from './followIssues';
import type { Issue, Repository } from './types';

const repository: Repository = {
  id: 7,
  github_id: 'github-7',
  full_name: 'acme/widget',
  owner: 'acme',
  name: 'widget',
  active: true,
  access_status: 'active',
};
const issue = (id: number): Issue => ({
  id,
  repository_id: 7,
  number: id,
  title: `Issue ${id}`,
  body: null,
  html_url: `https://github.com/acme/widget/issues/${id}`,
  labels_json: '[]',
  state: 'open',
  github_updated_at: '2026-09-26T00:00:00Z',
  version: `version-${id}`,
  full_name: 'acme/widget',
  active: true,
});

describe('follow issue observation', () => {
  it('baselines existing issues and returns only later issues', () => {
    const initial = observeFollowedIssues({}, [repository], [issue(1)]);
    expect(initial.newIssues).toEqual([]);

    const later = observeFollowedIssues(initial.state, [repository], [issue(1), issue(2)]);
    expect(later.newIssues.map(({ id }) => id)).toEqual([2]);
  });

  it('baselines again when follow issues is enabled', () => {
    const disabled: FollowState = { '7': { enabled: false, seenIssueIds: [] } };
    const enabled = observeFollowedIssues(disabled, [repository], [issue(1)]);
    expect(enabled.newIssues).toEqual([]);
    expect(enabled.state['7'].enabled).toBe(true);
  });

  it('can mark a successfully handled issue as seen', () => {
    const state: FollowState = { '7': { enabled: true, seenIssueIds: [] } };
    expect(markIssueSeen(state, issue(1))['7'].seenIssueIds).toEqual([1]);
  });
});
