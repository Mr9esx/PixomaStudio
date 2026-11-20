import { z } from 'zod'
import { newVideoShot, validateVideoProject, serializeVideoContent, type VideoProject } from './video-editor-model'
import { videoScriptSchema, videoScriptSourceSchema } from './video-script'

const sourcesSchema = z.array(videoScriptSourceSchema).refine(sources => new Set(sources.map(source => source.blockId)).size === sources.length)
const creativeFieldsSchema = z.object({ name: z.string().trim().min(1), storyboard: z.string(), scriptSources: sourcesSchema }).strict()
const changeSchema = z.discriminatedUnion('action', [
  z.object({ shotId: z.string().min(1), action: z.literal('add'), name: z.string().trim().min(1), storyboard: z.string(), sources: sourcesSchema }).strict(),
  z.object({ shotId: z.string().min(1), action: z.literal('update'), baseline: creativeFieldsSchema, name: z.string().trim().min(1), storyboard: z.string(), sources: sourcesSchema }).strict(),
  z.object({ shotId: z.string().min(1), action: z.literal('remove'), baseline: z.record(z.string(), z.unknown()), name: z.string().trim().min(1), storyboard: z.string(), sources: sourcesSchema }).strict().refine(change => change.baseline.id === change.shotId),
])
const orderSchema = z.array(z.string().min(1)).refine(ids => new Set(ids).size === ids.length)
export const videoCreativeProposalSchema = z.discriminatedUnion('kind', [
  z.object({ id: z.string().min(1), kind: z.literal('script'), baseline: videoScriptSchema, script: videoScriptSchema }).strict(),
  z.object({ id: z.string().min(1), kind: z.literal('storyboard'), order: orderSchema.optional(), reorder: z.object({ baseline: orderSchema, proposed: orderSchema }).strict().optional(), changes: z.array(changeSchema).max(100).refine(changes => new Set(changes.map(change => change.shotId)).size === changes.length) }).strict().refine(proposal => (proposal.changes.length > 0 || proposal.reorder !== undefined) && (!proposal.order || proposal.changes.filter(change => change.action === 'add').every(change => proposal.order!.includes(change.shotId)))),
])
export type VideoCreativeProposal = z.infer<typeof videoCreativeProposalSchema>

export function validateVideoCreativeProposals(project: VideoProject) {
  if (project.proposals === undefined) return
  if (!z.array(videoCreativeProposalSchema).safeParse(project.proposals).success) throw new Error('创作提案格式无效')
  if (new Set(project.proposals.map(proposal => proposal.id)).size !== project.proposals.length) throw new Error('创作提案标识重复')
}

export function applyVideoCreativeProposal(project: VideoProject, proposalId: string, selectedShotIds?: string[], applyOrder = false): VideoProject {
  validateVideoCreativeProposals(project)
  const proposal = project.proposals?.find(item => item.id === proposalId)
  if (!proposal) throw new Error('创作提案不存在')
  let next: VideoProject
  if (proposal.kind === 'script') {
    if (JSON.stringify(project.script) !== JSON.stringify(proposal.baseline)) throw new Error('脚本已修改，请根据当前脚本重新生成提案')
    next = { ...project, script: structuredClone(proposal.script), proposals: project.proposals!.filter(item => item.id !== proposalId) }
  } else {
    const selected = new Set(selectedShotIds ?? proposal.changes.map(change => change.shotId))
    if ((!selected.size && !(applyOrder && proposal.reorder)) || [...selected].some(id => !proposal.changes.some(change => change.shotId === id))) throw new Error('选择提案中的分镜')
    const shots = [...project.shots]
    const originalOrder = shots.map(shot => shot.id)
    if (applyOrder && proposal.reorder && JSON.stringify(originalOrder) !== JSON.stringify(proposal.reorder.baseline)) throw new Error('分镜顺序已修改，请重新查看顺序建议')
    const order = proposal.order ?? proposal.changes.filter(change => change.action === 'add').map(change => change.shotId)
    for (const change of proposal.changes.filter(change => selected.has(change.shotId))) {
      if (change.action !== 'remove' && change.sources.some(source => !project.script?.blocks.some(block => block.id === source.blockId && block.text === source.text && block.revision === source.revision))) throw new Error('脚本原文已修改，请重新生成分镜提案')
      const index = shots.findIndex(shot => shot.id === change.shotId)
      if (change.action === 'remove') {
        if (index < 0) throw new Error('目标分镜已被删除')
        if (serializeVideoContent(shots[index]) !== serializeVideoContent(change.baseline)) throw new Error('分镜内容已修改，请重新查看删除建议')
        shots.splice(index, 1)
      } else if (change.action === 'add') {
        if (index >= 0) throw new Error('新增分镜标识已存在')
        const position = order.indexOf(change.shotId)
        const following = order.slice(position + 1).find(id => shots.some(shot => shot.id === id))
        const previous = order.slice(0, position).reverse().find(id => shots.some(shot => shot.id === id))
        const insertAt = following ? shots.findIndex(shot => shot.id === following) : previous ? shots.findIndex(shot => shot.id === previous) + 1 : shots.length
        shots.splice(insertAt, 0, { ...newVideoShot(change.name), id: change.shotId, storyboard: change.storyboard, scriptSources: structuredClone(change.sources) })
      } else {
        if (index < 0) throw new Error('目标分镜已被删除')
        const shot = shots[index]
        if (shot.name !== change.baseline.name || (shot.storyboard ?? '') !== change.baseline.storyboard || JSON.stringify(shot.scriptSources ?? []) !== JSON.stringify(change.baseline.scriptSources)) throw new Error('分镜内容已修改，请根据当前分镜重新生成提案')
        shots[index] = { ...shot, name: change.name, storyboard: change.storyboard, scriptSources: structuredClone(change.sources) }
      }
    }
    const remaining = proposal.changes.filter(change => !selected.has(change.shotId))
    let reorder = proposal.reorder
    if (applyOrder && reorder) {
      if (reorder.proposed.length !== shots.length || reorder.proposed.some(id => !shots.some(shot => shot.id === id))) throw new Error('采用顺序前需要采用提案中的全部新增和删除')
      shots.sort((left, right) => reorder!.proposed.indexOf(left.id) - reorder!.proposed.indexOf(right.id))
      reorder = undefined
    } else if (reorder && JSON.stringify(originalOrder) === JSON.stringify(reorder.baseline)) {
      reorder = { ...reorder, baseline: shots.map(shot => shot.id) }
    }
    next = { ...project, shots, proposals: project.proposals!.flatMap(item => item.id !== proposalId ? [item] : remaining.length || reorder ? [{ ...proposal, order, reorder, changes: remaining }] : []) }
  }
  validateVideoProject(next)
  return next
}
