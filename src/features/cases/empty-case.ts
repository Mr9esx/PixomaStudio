import type { WorkflowEditorRecord } from './lib/workflow-record'

export function emptyCase(): WorkflowEditorRecord {
  return {
    name: '',
    category: 'image',
    node_id: '',
    inputs: [],
    outputs: [],
    bindings: { workflow: {}, inputs: [], outputs: [] },
    input_schema: { type: 'object', properties: {}, required: [], additionalProperties: false },
    workflow_filename: '',
    enabled: true,
  }
}
