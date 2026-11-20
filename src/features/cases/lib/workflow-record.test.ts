import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { deriveBindings, deriveInputSchema } from './derive'
import { parseWorkflow } from './workflow-parse'
import { buildWorkflowDefinition, type WorkflowEditorRecord } from './workflow-record'

const source = readFileSync(new URL('../../../../src-tauri/tests/data/klein9b-api.json', import.meta.url), 'utf8')

describe('导入验收工作流', () => {
  it('保留全部节点和绑定，保存本地计算节点与封面', () => {
    const parsed = parseWorkflow(source)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(parsed.graph.nodes).toHaveLength(9)
    expect(parsed.graph.api).toEqual(JSON.parse(source))
    const inputs = [{ key: 'prompt', type: 'string', required: true, node_id: '40', field_path: 'text' }]
    const outputs = [{ key: 'image', type: 'image', node_id: '80', index: 0 }]
    const bindings = deriveBindings(inputs, outputs)
    const record: WorkflowEditorRecord = {
      name: 'Klein9B 文生图', category: 'image', enabled: true, node_id: '13e9ad28-cc7c-488e-9aaf-557e12cde3a8',
      inputs, outputs, input_schema: deriveInputSchema(inputs),
      bindings: { workflow: parsed.graph.api, ...bindings },
      preview: 'pixoma-media://localhost/asset/version#result.png', workflow_filename: 'MiniMax_Klein9B_文生图_本地.json',
    }
    const definition = buildWorkflowDefinition(record)
    expect(definition.execution.workflow).toEqual(JSON.parse(source))
    expect(definition.execution.node_id).toBe(record.node_id)
    expect(definition.execution.input_bindings).toEqual(bindings.inputs)
    expect(definition.execution.output_bindings).toEqual(bindings.outputs)
    expect(definition.preview).toBe(record.preview)
    expect(definition.input_schema).toEqual(record.input_schema)
  })

  it('媒体输入使用明确的资产和版本引用', () => {
    const schema = deriveInputSchema([{ key: 'reference', type: 'image', required: true, node_id: '10', field_path: 'image' }])
    expect(schema.properties).toEqual({ reference: {
      type: 'object', properties: { asset_id: { type: 'string', minLength: 1 }, asset_version_id: { type: 'string', minLength: 1 } },
      required: ['asset_id', 'asset_version_id'], additionalProperties: false,
    } })
  })
})
