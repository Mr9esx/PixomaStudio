import type { StudioWorkflowExecution } from '@/lib/api/studio'

export function isActiveWorkflowExecution(execution: StudioWorkflowExecution) {
  return execution.status === 'submitted' && !['succeeded', 'failed', 'cancelled', 'result_unknown'].includes(execution.task_status ?? '')
}

export function workflowExecutionTitle(execution: StudioWorkflowExecution) {
  return execution.workflow.name
}
