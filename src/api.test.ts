import { describe, expect, it } from 'vitest';
import { createApi } from './api';
describe('Worker API mapping', () => {
  it('uses exact approval and claim payloads', async () => {
    const calls: any[] = [];
    const api = createApi(async (p, i) => {
      calls.push([p, i]);
      return {};
    });
    await api.approve(7, 'v1', 'stable-key');
    await api.claim('job-1', 'client', '550e8400-e29b-41d4-a716-446655440000');
    expect(calls[0][0]).toBe('/v1/issues/7/approve');
    expect(calls[0][1].headers).toEqual({ 'Idempotency-Key': 'stable-key' });
    expect(calls[1][1].body).toContain('claim_id');
  });
});
describe('Worker API mapping', () => {
  it('uses explicit claim validation, heartbeat and terminal status endpoints', async () => {
    const calls: any[] = [];
    const api = createApi(async (p, i) => {
      calls.push([p, i]);
      return {};
    });
    await api.claimStatus('job-1', '550e8400-e29b-41d4-a716-446655440000');
    await api.heartbeat('job-1', '550e8400-e29b-41d4-a716-446655440000');
    await api.status('job-1', {
      claim_id: '550e8400-e29b-41d4-a716-446655440000',
      status: 'succeeded',
    });
    expect(calls.map(([path]) => path)).toEqual([
      '/v1/jobs/job-1/claim?claim_id=550e8400-e29b-41d4-a716-446655440000',
      '/v1/jobs/job-1/heartbeat',
      '/v1/jobs/job-1/status',
    ]);
    expect(calls[1][1].body).toContain('claim_id');
    expect(calls[2][1].body).toContain('succeeded');
  });
});

describe('Worker pagination', () => {
  it('loads records beyond the first page for repositories, issues, and jobs', async () => {
    const paths: string[] = [];
    const api = createApi(async (path) => {
      paths.push(path);
      return path.includes('cursor=')
        ? { items: [{ id: 101 }], next_cursor: null }
        : { items: [{ id: 1 }], next_cursor: 'next+page/=' };
    });
    for (const result of [
      await api.repositories(),
      await api.issues(7),
      await api.jobs('queued'),
    ]) {
      expect(result.items.map((item) => item.id)).toEqual([1, 101]);
    }
    expect(paths[1]).toContain('cursor=next%2Bpage%2F%3D');
    expect(paths[3]).toContain('repository_id=7&cursor=');
    expect(paths[5]).toContain('status=queued&cursor=');
  });
  it('fails visibly if the Worker repeats a cursor', async () => {
    const api = createApi(async () => ({ items: [], next_cursor: 'same' }));
    await expect(api.repositories()).rejects.toThrow('repeated');
  });
});
