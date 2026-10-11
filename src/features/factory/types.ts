export type FeatureStatus =
  | 'drafting'
  | 'planning'
  | 'awaiting_specification_approval'
  | 'queued'
  | 'implementing'
  | 'validating'
  | 'awaiting_delivery_approval'
  | 'delivered'
  | 'blocked'
  | 'cancelled';
export type Task = {
  id: string;
  repositoryId: number;
  title: string;
  instructions: string;
  dependencies: string[];
  acceptanceCriteria: string[];
};
export type Specification = {
  goals: string[];
  exclusions: string[];
  acceptanceCriteria: string[];
  repositoryChanges: string[];
  tasks: Task[];
  checks: { repositoryId: number; command: string }[];
  questions: string[];
};
export type RepositorySnapshot = {
  repositoryId: number;
  localPath: string;
  baseBranch: string;
  baseSha: string;
  testCommand: string;
  remote: string;
  branch: string;
  worktree: string;
  headSha: string | null;
  validatedTree: string | null;
  prUrl: string | null;
  ready: boolean;
};
export type FeatureRevision = {
  id: string;
  specification: Specification;
  repositories: RepositorySnapshot[];
};
export type Message = { id: string; role: string; text: string; createdAt: string };
export type Run = {
  id: string;
  role: string;
  taskId: string | null;
  status: string;
  createdAt: string;
  logPath: string;
};
export type Artifact = { runId: string; kind: string; text: string };
export type Approval = {
  kind: string;
  revisionId: string;
  commitShas: string[];
  createdAt: string;
};
export type ValidationResult = {
  id: string;
  taskId: string | null;
  repositoryId: number;
  command: string;
  passed: boolean;
  output: string;
  createdAt: string;
};
export type Feature = {
  id: string;
  title: string;
  brief: string;
  sourceIssueId: number | null;
  repositoryIds: number[];
  status: FeatureStatus;
  messages: Message[];
  draft: FeatureRevision | null;
  approvedRevisionId: string | null;
  repositories: RepositorySnapshot[];
  approvals: Approval[];
  completedTasks: string[];
  repairCounts: Record<string, number>;
  feedbackCursors: Record<string, string>;
  runs: Run[];
  artifacts: Artifact[];
  validations: ValidationResult[];
  operations: {
    repositoryId: number;
    branch: string;
    kind: string;
    sha: string;
    completed: boolean;
  }[];
  events: Message[];
  error: string | null;
  paused: boolean;
  createdAt: string;
};
export type CreateFeature = {
  title: string;
  brief: string;
  repositoryIds: number[];
  sourceIssueId: number | null;
};
