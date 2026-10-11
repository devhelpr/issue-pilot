import { describe, expect, it } from 'vitest';
import { createFactoryApi, parseSpecification } from './api';
import type { Feature } from './types';

describe('factory approval boundary', () => {
  it('binds delivery to the reviewed revision and repository commits', async () => {
    const calls: { command: string; args?: Record<string, unknown> }[] = [];
    const api = createFactoryApi(async <T>(command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      return undefined as T;
    });
    const feature = {
      id: 'feature',
      approvedRevisionId: 'reviewed-revision',
      repositories: [{ headSha: 'api-sha' }, { headSha: 'ui-sha' }],
    } as Feature;
    await api.approveDelivery(feature);
    expect(calls).toEqual([
      {
        command: 'approve_feature_delivery',
        args: {
          id: 'feature',
          revisionId: 'reviewed-revision',
          commitShas: ['api-sha', 'ui-sha'],
        },
      },
    ]);
  });
  it('carries explicit recovery acknowledgement and cancellation intent', async () => {
    const calls: Record<string, unknown>[] = [];
    const api = createFactoryApi(async <T>(_command: string, args?: Record<string, unknown>) => {
      calls.push(args ?? {});
      return undefined as T;
    });
    await api.execute('feature', true);
    await api.control('feature', 'cancel');
    expect(calls).toEqual([
      { id: 'feature', inspected: true },
      { id: 'feature', action: 'cancel' },
    ]);
  });
  it('rejects malformed edited specifications before calling the backend', () => {
    for (const input of ['null', '[]', '{}', '{"tasks": "invalid"}', '{"tasks": []}']) {
      expect(() => parseSpecification(input)).toThrow();
    }
  });
});
