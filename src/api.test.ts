import { describe, expect, it } from 'vitest';
import { createApi } from './api';
describe('Worker API mapping', () => { it('uses exact approval and claim payloads', async () => { const calls: any[] = []; const api = createApi(async (p, i) => { calls.push([p, i]); return {}; }); await api.approve(7, 'v1', 'stable-key'); await api.claim('job-1', 'client', '550e8400-e29b-41d4-a716-446655440000'); expect(calls[0][0]).toBe('/v1/issues/7/approve'); expect(calls[0][1].headers).toEqual({ 'Idempotency-Key': 'stable-key' }); expect(calls[1][1].body).toContain('claim_id'); }); });
