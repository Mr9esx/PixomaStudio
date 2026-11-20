import { z } from 'zod'
import type { VideoProject, VideoShot } from './video-editor-model'

const scriptBlockSchema = z.object({ id: z.string().min(1), text: z.string(), revision: z.number().int().positive(), ancestorIds: z.array(z.string().min(1)).optional() })
export const videoScriptSchema = z.object({ revision: z.number().int().nonnegative(), blocks: z.array(scriptBlockSchema) }).strict().refine(script => {
  const ids = new Set(script.blocks.map(block => block.id))
  return ids.size === script.blocks.length && script.blocks.every(block => block.revision <= script.revision && (!block.ancestorIds || (new Set(block.ancestorIds).size === block.ancestorIds.length && block.ancestorIds.every(id => !ids.has(id)))))
})
export const videoScriptSourceSchema = z.object({ blockId: z.string().min(1), text: z.string(), revision: z.number().int().positive() }).strict()
export type VideoScript = z.infer<typeof videoScriptSchema>
export type VideoScriptSource = z.infer<typeof videoScriptSourceSchema>
export const videoScriptSourceLabels = { unlinked: '手动创建', current: '', changed: '原文已修改', missing: '原文已删除', restructured: '检查原文关联' } as const

function validateNewBlockIds(project: VideoProject, ids: string[]) {
  const existing = new Set([...(project.script?.blocks.flatMap(block => [block.id, ...(block.ancestorIds ?? [])]) ?? []), ...project.shots.flatMap(shot => shot.scriptSources?.map(source => source.blockId) ?? [])])
  if (ids.some(id => !id.trim())) throw new Error('脚本文本块标识不能为空')
  if (new Set(ids).size !== ids.length || ids.some(id => existing.has(id))) throw new Error('脚本文本块标识重复')
}

export function videoScriptSuccessors(script: VideoScript, blockId: string) {
  return script.blocks.filter(block => block.id === blockId || block.ancestorIds?.includes(blockId))
}

export function splitVideoScriptBlock(project: VideoProject, id: string, position: number, ids: [string, string]): VideoProject {
  const current = normalizeProjectScript(project)
  const script = current.script!
  const target = script.blocks.find(block => block.id === id)
  if (!target) throw new Error('脚本文本块不存在')
  if (!Number.isInteger(position) || position <= 0 || position >= target.text.length || !target.text.slice(0, position).trim() || !target.text.slice(position).trim()) throw new Error('选择文本内部的分开位置')
  validateNewBlockIds(current, ids)
  const ancestorIds = [...(target.ancestorIds ?? []), target.id]
  const blocks = ids.map((id, index) => ({ id, text: index ? target.text.slice(position) : target.text.slice(0, position), revision: 1, ancestorIds: [...ancestorIds] }))
  return { ...current, script: { ...script, revision: script.revision + 1, blocks: script.blocks.flatMap(block => block.id === id ? blocks : [block]) } }
}

export function mergeVideoScriptBlocks(project: VideoProject, ids: string[], id: string): VideoProject {
  const current = normalizeProjectScript(project)
  const script = current.script!
  const index = script.blocks.findIndex(block => block.id === ids[0])
  if (ids.length < 2 || index < 0 || ids.some((value, offset) => script.blocks[index + offset]?.id !== value)) throw new Error('选择按顺序相邻的脚本文本块')
  validateNewBlockIds(current, [id])
  const targets = script.blocks.slice(index, index + ids.length)
  const ancestorIds = [...new Set([...targets.flatMap(block => block.ancestorIds ?? []), ...ids])]
  const merged = { id, text: targets.map(block => block.text).join('\n\n'), revision: 1, ancestorIds }
  return { ...current, script: { ...script, revision: script.revision + 1, blocks: [...script.blocks.slice(0, index), merged, ...script.blocks.slice(index + ids.length)] } }
}

export function normalizeProjectScript(project: VideoProject): VideoProject {
  if (project.script) return project
  const blocks = project.shots.filter(shot => shot.script.trim()).map(shot => ({ id: `script-${shot.id}`, text: shot.script, revision: 1 }))
  return { ...project, script: { revision: blocks.length ? 1 : 0, blocks }, shots: project.shots.map(shot => ({ ...shot,
    scriptSources: shot.scriptSources ?? (shot.script.trim() ? [{ blockId: `script-${shot.id}`, text: shot.script, revision: 1 }] : []),
  })) }
}

export function validateVideoScript(project: VideoProject): void {
  if (project.script) {
    if (!videoScriptSchema.safeParse(project.script).success) throw new Error('脚本内容或修订号无效')
    if (new Set(project.script.blocks.map(block => block.id)).size !== project.script.blocks.length) throw new Error('脚本文本块标识重复')
    if (project.script.blocks.some(block => block.revision > project.script!.revision)) throw new Error('脚本修订号无效')
  }
  for (const shot of project.shots) {
    if (shot.scriptSources === undefined) continue
    if (!z.array(videoScriptSourceSchema).safeParse(shot.scriptSources).success) throw new Error('分镜原文关联无效')
    if (new Set(shot.scriptSources.map(source => source.blockId)).size !== shot.scriptSources.length) throw new Error('分镜原文关联重复')
  }
}

export function addVideoScriptBlock(project: VideoProject, id: string, text = ''): VideoProject {
  const current = normalizeProjectScript(project)
  const script = current.script!
  validateNewBlockIds(current, [id])
  return { ...current, script: { revision: script.revision + 1, blocks: [...script.blocks, { id, text, revision: 1 }] } }
}

export function importVideoScript(project: VideoProject, baseline: VideoScript, id: string, content: string, mode: 'append' | 'replace'): VideoProject {
  const current = normalizeProjectScript(project)
  if (JSON.stringify(current.script) !== JSON.stringify(baseline)) throw new Error('脚本已修改，请重新查看文档后采用')
  if (!content.trim()) throw new Error('文档没有可用的脚本内容')
  validateNewBlockIds(current, [id])
  return { ...current, script: { revision: current.script!.revision + 1, blocks: [...(mode === 'append' ? current.script!.blocks : []), { id, text: content, revision: 1 }] } }
}

export function updateVideoScriptBlock(project: VideoProject, id: string, text: string): VideoProject {
  const current = normalizeProjectScript(project)
  const script = current.script!
  const target = script.blocks.find(block => block.id === id)
  if (!target) throw new Error('脚本文本块不存在')
  if (target.text === text) return current
  return { ...current, script: { revision: script.revision + 1, blocks: script.blocks.map(block => block.id === id ? { ...block, text, revision: block.revision + 1 } : block) } }
}

export function removeVideoScriptBlock(project: VideoProject, id: string): VideoProject {
  const current = normalizeProjectScript(project)
  const script = current.script!
  if (!script.blocks.some(block => block.id === id)) throw new Error('脚本文本块不存在')
  return { ...current, script: { revision: script.revision + 1, blocks: script.blocks.filter(block => block.id !== id) } }
}

export function setVideoScriptSources(shot: VideoShot, script: VideoScript, ids: string[]): VideoShot {
  if (new Set(ids).size !== ids.length) throw new Error('分镜原文关联重复')
  return { ...shot, scriptSources: ids.map(id => {
    const block = script.blocks.find(block => block.id === id)
    if (!block) throw new Error('脚本文本块不存在')
    return { blockId: block.id, text: block.text, revision: block.revision }
  }) }
}

export function videoScriptSourceState(shot: VideoShot, script: VideoScript): 'unlinked' | 'current' | 'changed' | 'missing' | 'restructured' {
  if (!shot.scriptSources?.length) return 'unlinked'
  if (shot.scriptSources.some(source => !videoScriptSuccessors(script, source.blockId).length)) return 'missing'
  if (shot.scriptSources.some(source => !script.blocks.some(block => block.id === source.blockId))) return 'restructured'
  return shot.scriptSources.some(source => script.blocks.find(block => block.id === source.blockId)!.text !== source.text) ? 'changed' : 'current'
}
