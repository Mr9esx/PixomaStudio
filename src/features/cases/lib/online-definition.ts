import type { OnlineWorkflowDefinition } from '@/lib/api/workflows'
import { z } from 'zod'

export function parseOnlineDefinition(source: string): OnlineWorkflowDefinition {
  const object = z.record(z.string(), z.unknown())
  return z.object({ type: z.literal('online'),
    inputs: z.array(z.object({ key: z.string().min(1), type: z.enum(['string', 'number', 'boolean', 'enum', 'image', 'video', 'audio', 'file']), required: z.boolean() }).passthrough()),
    outputs: z.array(z.object({ key: z.string().min(1), type: z.enum(['image', 'video', 'audio', 'text', 'file']) }).passthrough()), input_schema: object,
    execution: z.object({ provider: z.enum(['seedance', 'seedream', 'minimax', 'http_json']), operation: z.enum(['video', 'image']), connection_id: z.string(), model: z.string().min(1), capability: z.string().min(1), defaults: object, input_mapping: z.record(z.string(), z.string()), output_mapping: z.record(z.string(), z.string()) }).passthrough(),
  }).passthrough().parse(JSON.parse(source)) as OnlineWorkflowDefinition
}

export function prepareOnlineDefinition(definition: OnlineWorkflowDefinition, exposed: string[]): OnlineWorkflowDefinition {
  if (exposed.some(key => !definition.inputs.some(field => field.key === key))) throw new Error('开放参数不存在')
  const inputs = definition.inputs.filter(field => exposed.includes(field.key))
  for (const field of definition.inputs) {
    if (!field.required || exposed.includes(field.key)) continue
    const value = definition.execution.defaults[field.key]
    if (value === undefined || value === null || value === '') throw new Error(`${field.description || field.key}需要默认值或开放输入`)
  }
  const properties = definition.input_schema.properties as Record<string, unknown>
  return { ...definition, inputs,
    input_schema: { ...definition.input_schema, properties: Object.fromEntries(inputs.map(field => [field.key, properties[field.key]])), required: inputs.filter(field => field.required).map(field => field.key) },
    execution: { ...definition.execution, input_mapping: Object.fromEntries(Object.entries(definition.execution.input_mapping).filter(([key]) => exposed.includes(key) || definition.execution.defaults[key] !== undefined)) },
  }
}
