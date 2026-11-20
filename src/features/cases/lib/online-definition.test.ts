import { expect, it } from 'vitest'
import type { OnlineWorkflowDefinition } from '@/lib/api/workflows'
import { parseOnlineDefinition, prepareOnlineDefinition } from './online-definition'

it('固定参数保留默认值与参数映射，必填参数需要有值', () => {
  const definition: OnlineWorkflowDefinition = {
    type: 'online', inputs: [{ key: 'prompt', type: 'string', required: true }, { key: 'duration', type: 'number', required: true }],
    input_schema: { type: 'object', properties: { prompt: { type: 'string' }, duration: { type: 'integer', minimum: 4, maximum: 15 } }, required: ['prompt', 'duration'] },
    outputs: [{ key: 'video', type: 'video' }], execution: { provider: 'seedance', operation: 'video', connection_id: '', model: 'doubao-seedance-2-0-260128', capability: 'text_to_video', defaults: { duration: 4 }, input_mapping: { prompt: 'prompt', duration: 'duration' }, output_mapping: { video: 'video_url' } },
  }
  const result = prepareOnlineDefinition(definition, ['prompt'])
  expect(result.inputs.map(field => field.key)).toEqual(['prompt'])
  expect(result.execution.defaults.duration).toBe(4)
  expect(result.execution.input_mapping).toEqual({ prompt: 'prompt', duration: 'duration' })
  expect(result.input_schema.properties).toEqual({ prompt: { type: 'string' } })
  expect(definition.inputs).toHaveLength(2)
  expect(parseOnlineDefinition(JSON.stringify(result))).toEqual(result)
  expect(() => parseOnlineDefinition('{')).toThrow()
  expect(() => parseOnlineDefinition(JSON.stringify({ ...definition, inputs: [{ key: 'prompt', type: 'unsupported', required: true }] }))).toThrow()
  expect(() => prepareOnlineDefinition(definition, ['duration'])).toThrow('需要默认值或开放输入')
  expect(() => prepareOnlineDefinition(definition, ['missing'])).toThrow('开放参数不存在')
})
