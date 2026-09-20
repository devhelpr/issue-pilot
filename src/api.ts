import type { Issue, Job, Repository } from './types';

export class ApiError extends Error { constructor(public status: number, message: string) { super(message); } }
type Requester = (path: string, init?: RequestInit) => Promise<unknown>;
export const createApi = (request: Requester) => ({
  repositories: () => request('/v1/repositories?limit=100') as Promise<{items: Repository[]}>,
  syncGithub: () => request('/v1/github/sync', { method: 'POST' }),
  setRepository: (id: number, active: boolean) => request(`/v1/repositories/${id}`, { method: 'PATCH', body: JSON.stringify({ active }) }),
  syncRepository: (id: number) => request(`/v1/repositories/${id}/sync`, { method: 'POST' }),
  issues: (repositoryId?: number, state = 'open') => request(`/v1/issues?state=${state}&limit=100${repositoryId ? `&repository_id=${repositoryId}` : ''}`) as Promise<{items: Issue[]}>,
  approve: (id: number, version: string, key: string) => request(`/v1/issues/${id}/approve`, { method: 'POST', headers: { 'Idempotency-Key': key }, body: JSON.stringify({ version }) }),
  jobs: (status?: string) => request(`/v1/jobs?limit=100${status ? `&status=${status}` : ''}`) as Promise<{items: Job[]}>,
  job: (id: string) => request(`/v1/jobs/${id}`) as Promise<Job>,
  claim: (id: string, client_id: string, claim_id: string) => request(`/v1/jobs/${id}/claim`, { method: 'POST', body: JSON.stringify({ client_id, claim_id }) }),
  claimStatus: (id: string, claimId: string) => request(`/v1/jobs/${id}/claim?claim_id=${encodeURIComponent(claimId)}`) as Promise<{valid: boolean; stop_requested: boolean; repository_active: boolean; issue_state: string}>,
  heartbeat: (id: string, claimId: string) => request(`/v1/jobs/${id}/heartbeat`, { method: 'POST', body: JSON.stringify({ claim_id: claimId }) }),
  status: (id: string, body: Record<string, unknown>) => request(`/v1/jobs/${id}/status`, { method: 'POST', body: JSON.stringify(body) }),
  retry: (id: string) => request(`/v1/jobs/${id}/retry`, { method: 'POST' })
});
