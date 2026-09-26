export type Repository = {
  id: number;
  github_id: string;
  full_name: string;
  owner: string;
  name: string;
  active: number | boolean;
  access_status: 'active' | 'revoked';
  last_synced_at?: string | null;
};
export type Issue = {
  id: number;
  repository_id: number;
  number: number;
  title: string;
  body: string | null;
  html_url: string;
  labels_json: string;
  state: 'open' | 'closed';
  github_updated_at: string;
  version: string;
  full_name: string;
  active: number | boolean;
};
export type Job = {
  id: string;
  issue_id: number;
  issue_version: string;
  issue_title: string;
  issue_body: string | null;
  status: string;
  phase?: string | null;
  claim_id?: string | null;
  stop_requested?: number | boolean;
  result_summary?: string | null;
  commit_sha?: string | null;
  pr_url?: string | null;
  events?: unknown[];
};
export type JobDebugEvent = {
  id: number;
  job_id: string;
  phase: string | null;
  level: 'info' | 'error' | 'warning';
  message: string;
  detail: string | null;
  created_at: string;
};
export type Settings = {
  workerUrl: string;
  hasToken: boolean;
  clientId: string;
  codexPath: string;
  gitPath: string;
  ghPath: string;
};
