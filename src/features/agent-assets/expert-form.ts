import { z } from 'zod'
import { prerelease, valid } from 'semver'

const stableVersion = z.string().refine(value => valid(value) === value && prerelease(value) === null, '版本号需为 x.y.z')
export const expertPartSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('text'), text: z.string().min(1, '填写执行说明') }),
  z.object({ type: z.literal('skill'), skill_id: z.string().min(1), version: stableVersion, name: z.string().optional() }),
  z.object({ type: z.literal('workflow'), workflow_id: z.string().min(1), version_id: z.string().min(1), name: z.string().optional() }),
])
export type ExpertPart = z.infer<typeof expertPartSchema>

export const expertFormSchema = z.object({
  schema_version: z.literal(2),
  name: z.string().trim().min(1, '填写专家名称').max(256),
  description: z.string(),
  model_id: z.string().min(1, '选择模型'),
  prompt_parts: z.array(expertPartSchema).min(1, '填写专家提示词'),
  avatar_asset_id: z.string(),
  avatar_version_id: z.string(),
  examples: z.array(z.string()),
  skill_refs: z.array(z.object({ skill_id: z.string(), version: stableVersion })),
  workflow_refs: z.array(z.object({ workflow_id: z.string(), version_id: z.string().min(1) })),
  sop: z.array(z.object({ id: z.string().min(1), title: z.string().trim().min(1, '填写步骤名称'), instruction_parts: z.array(expertPartSchema).min(1, '填写步骤说明') })),
}).superRefine((value, ctx) => {
  if (Boolean(value.avatar_asset_id) !== Boolean(value.avatar_version_id)) ctx.addIssue({ code: 'custom', path: ['avatar_version_id'], message: '选择头像版本' })
  const steps = new Set<string>()
  value.sop.forEach((step, index) => {
    if (steps.has(step.id)) ctx.addIssue({ code: 'custom', path: ['sop', index, 'id'], message: '步骤标识重复' })
    steps.add(step.id)
  })
  const versions = new Map<string, string>()
  const sections = [{ path: ['prompt_parts'], parts: value.prompt_parts }, ...value.sop.map((step, index) => ({ path: ['sop', index, 'instruction_parts'], parts: step.instruction_parts }))]
  sections.forEach(section => section.parts.forEach((part, index) => {
    if (part.type === 'text') return
    const key = part.type === 'skill' ? `skill:${part.skill_id}` : `workflow:${part.workflow_id}`
    const version = part.type === 'skill' ? part.version : part.version_id
    if (versions.has(key) && versions.get(key) !== version) ctx.addIssue({ code: 'custom', path: [...section.path, index], message: '同一能力需要引用相同版本' })
    versions.set(key, version)
  }))
})
export type ExpertContent = z.infer<typeof expertFormSchema>

export function expertReferencedCapabilities(content: Pick<ExpertContent, 'prompt_parts' | 'sop'>) {
  const refs = new Map<string, Exclude<ExpertPart, { type: 'text' }>>()
  for (const part of [...content.prompt_parts, ...content.sop.flatMap(step => step.instruction_parts)]) {
    if (part.type !== 'text') refs.set(part.type === 'skill' ? `skill:${part.skill_id}` : `workflow:${part.workflow_id}`, part)
  }
  return [...refs.values()]
}

export function expertPartsText(parts: ExpertPart[]) {
  return parts.map(part => part.type === 'text' ? part.text : part.type === 'skill' ? `@${part.name ?? part.skill_id} · v${part.version}` : `@${part.name ?? part.workflow_id} · #${part.version_id}`).join('')
}
