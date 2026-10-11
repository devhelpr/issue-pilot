import { invoke } from '@tauri-apps/api/core';
import type { CreateFeature, Feature, Specification } from './types';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export const createFactoryApi = (request: Invoke) => ({
  list: () => request<Feature[]>('list_features'),
  create: (input: CreateFeature) => request<Feature>('create_feature', { input }),
  plan: (id: string, message: string) => request<Feature>('plan_feature', { id, message }),
  save: (id: string, specification: Specification) =>
    request<Feature>('save_feature_revision', { id, specification }),
  approveSpecification: (id: string, revisionId: string) =>
    request<Feature>('approve_feature_revision', { id, revisionId }),
  execute: (id: string, inspected: boolean) => request<void>('execute_feature', { id, inspected }),
  control: (id: string, action: 'pause' | 'cancel') =>
    request<void>('control_feature', { id, action }),
  approveDelivery: (feature: Feature) =>
    request<void>('approve_feature_delivery', {
      id: feature.id,
      revisionId: feature.approvedRevisionId,
      commitShas: feature.repositories.flatMap((repository) =>
        repository.headSha ? [repository.headSha] : [],
      ),
    }),
  syncFeedback: (id: string) => request<Feature>('sync_feature_feedback', { id }),
  feedback: (id: string, text: string) => request<Feature>('feature_feedback', { id, text }),
});
export const factoryApi = createFactoryApi(invoke);
export const isActive = (feature: Feature) =>
  ['planning', 'queued', 'implementing', 'validating'].includes(feature.status) ||
  feature.runs.some((run) => run.status === 'running');

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function strings(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === 'string');
}
function isSpecification(value: unknown): value is Specification {
  if (!record(value)) return false;
  const lists = ['goals', 'exclusions', 'acceptanceCriteria', 'repositoryChanges', 'questions'];
  if (!lists.every((key) => strings(value[key]))) return false;
  if (!Array.isArray(value.tasks) || !Array.isArray(value.checks)) return false;
  return (
    value.tasks.every(
      (task: unknown) =>
        record(task) &&
        typeof task.id === 'string' &&
        Number.isInteger(task.repositoryId) &&
        typeof task.title === 'string' &&
        typeof task.instructions === 'string' &&
        strings(task.dependencies) &&
        strings(task.acceptanceCriteria),
    ) &&
    value.checks.every(
      (check: unknown) =>
        record(check) && Number.isInteger(check.repositoryId) && typeof check.command === 'string',
    )
  );
}
export function parseSpecification(text: string): Specification {
  const value: unknown = JSON.parse(text);
  if (!isSpecification(value)) {
    throw new Error(
      'The specification must contain goals, exclusions, acceptance criteria, repository changes, questions, tasks, and checks with valid field types.',
    );
  }
  return value;
}
