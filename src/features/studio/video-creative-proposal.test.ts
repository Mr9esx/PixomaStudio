import { expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, normalizeVideoProject } from './video-editor-model'
import { applyVideoCreativeProposal, type VideoCreativeProposal } from './video-creative-proposal'
import { updateVideoScriptBlock } from './video-script'

it('删除建议需要明确采用，保留脚本并在分镜发生变化后拒绝删除', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾', '晨雾散开'), newVideoShot('茶杯', '捧起茶杯')] })
  const shot = original.shots[0]
  const proposal: VideoCreativeProposal = { id: 'remove', kind: 'storyboard', changes: [{ action: 'remove', shotId: shot.id, name: shot.name, storyboard: shot.storyboard ?? '', sources: shot.scriptSources!, baseline: structuredClone(shot) }] }
  const project = { ...original, proposals: [proposal] }
  const next = applyVideoCreativeProposal(project, proposal.id, [shot.id])
  expect(next.shots).toEqual([original.shots[1]])
  expect(next.script).toEqual(original.script)
  expect(() => applyVideoCreativeProposal({ ...project, shots: [{ ...shot, inputs: { prompt: '新参数' } }, original.shots[1]] }, proposal.id, [shot.id])).toThrow('分镜内容已修改')
})

it('顺序建议单独采用，分批内容采用保留顺序建议并检查并发排序', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾'), newVideoShot('茶杯'), newVideoShot('窗户')] })
  const shot = original.shots[0]
  const baseline = original.shots.map(shot => shot.id)
  const order = [baseline[2], baseline[0], baseline[1]]
  const proposal: VideoCreativeProposal = { id: 'order', kind: 'storyboard', order, reorder: { baseline, proposed: order }, changes: [{ action: 'update', shotId: shot.id, baseline: { name: shot.name, storyboard: '', scriptSources: [] }, name: '清晨', storyboard: '薄雾', sources: [] }] }
  const project = { ...original, proposals: [proposal] }
  const content = applyVideoCreativeProposal(project, proposal.id, [shot.id])
  expect(content.shots.map(shot => shot.id)).toEqual(baseline)
  expect(content.proposals).toHaveLength(1)
  const reordered = applyVideoCreativeProposal(content, proposal.id, [], true)
  expect(reordered.shots.map(shot => shot.id)).toEqual(order)
  expect(reordered.shots[1].name).toBe('清晨')
  expect(reordered.proposals).toEqual([])
  expect(() => applyVideoCreativeProposal({ ...content, shots: [...content.shots].reverse() }, proposal.id, [], true)).toThrow('分镜顺序已修改')
})

it('采用分镜提案只修改选定创作内容，保留采用视频与生成参数', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾', '晨雾散开'), newVideoShot('茶杯', '捧起茶杯')] })
  const shot = { ...original.shots[0], duration: 3, inputs: { prompt: '手工参数' }, media: { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 5 } }
  const change = { shotId: shot.id, action: 'update' as const, baseline: { name: shot.name, storyboard: shot.storyboard ?? '', scriptSources: shot.scriptSources ?? [] }, name: '清晨茶园', storyboard: '阳光照亮茶园', sources: shot.scriptSources! }
  const proposal: VideoCreativeProposal = { id: 'proposal', kind: 'storyboard', changes: [change] }
  const project = { ...original, shots: [shot, original.shots[1]], proposals: [proposal] }
  const next = applyVideoCreativeProposal(project, 'proposal', [shot.id])
  expect(next.shots[0]).toMatchObject({ name: '清晨茶园', storyboard: '阳光照亮茶园', media: shot.media, inputs: shot.inputs, duration: shot.duration })
  expect(next.shots[1]).toEqual(project.shots[1])
  expect(next.proposals).toEqual([])
  expect(project.shots[0].name).toBe('晨雾')
})

it('原文或分镜描述在提案生成后修改，拒绝覆盖并保留提案', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾', '晨雾散开')] })
  const shot = original.shots[0]
  const proposal: VideoCreativeProposal = { id: 'proposal', kind: 'storyboard', changes: [{ shotId: shot.id, action: 'update', baseline: { name: shot.name, storyboard: '', scriptSources: shot.scriptSources! }, name: '新名称', storyboard: '生成描述', sources: shot.scriptSources! }] }
  const project = { ...original, proposals: [proposal] }
  const changed = updateVideoScriptBlock(project, project.script!.blocks[0].id, '手动修改原文')
  expect(() => applyVideoCreativeProposal(changed, proposal.id, [shot.id])).toThrow('原文')
  expect(() => applyVideoCreativeProposal({ ...project, shots: [{ ...shot, storyboard: '手动描述' }] }, proposal.id, [shot.id])).toThrow('分镜')
  expect(changed.proposals).toEqual([proposal])
})

it('分批采用提案保留尚未采用的修改，新增分镜具有稳定来源', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾', '晨雾散开')] })
  const shot = original.shots[0]
  const proposal: VideoCreativeProposal = { id: 'proposal', kind: 'storyboard', changes: [
    { shotId: shot.id, action: 'update', baseline: { name: shot.name, storyboard: '', scriptSources: shot.scriptSources! }, name: '修改名称', storyboard: '修改描述', sources: shot.scriptSources! },
    { shotId: 'new-shot', action: 'add', name: '第二个分镜', storyboard: '近景', sources: shot.scriptSources! },
  ] }
  const first = applyVideoCreativeProposal({ ...original, proposals: [proposal] }, proposal.id, ['new-shot'])
  expect(first.shots[0]).toEqual(shot)
  expect(first.shots[1]).toMatchObject({ id: 'new-shot', scriptSources: shot.scriptSources })
  expect(first.proposals?.[0].kind === 'storyboard' && first.proposals[0].changes).toHaveLength(1)
  expect(applyVideoCreativeProposal(first, proposal.id, [shot.id]).proposals).toEqual([])
})

it('完整脚本提案采用后保留分镜，并拒绝覆盖期间修改', () => {
  const original = normalizeVideoProject({ ...emptyVideoProject(), shots: [newVideoShot('晨雾', '晨雾散开')] })
  const proposal: VideoCreativeProposal = { id: 'script-proposal', kind: 'script', baseline: original.script!, script: { revision: 2, blocks: [{ ...original.script!.blocks[0], text: '阳光穿过薄雾', revision: 2 }] } }
  const project = { ...original, proposals: [proposal] }
  const next = applyVideoCreativeProposal(project, proposal.id)
  expect(next.script).toEqual(proposal.script)
  expect(next.shots).toEqual(project.shots)
  const changed = updateVideoScriptBlock(project, original.script!.blocks[0].id, '手动输入')
  expect(() => applyVideoCreativeProposal(changed, proposal.id)).toThrow('脚本')
})

it('分批采用新增分镜仍然遵循提案中的顺序', () => {
  const original = normalizeVideoProject(emptyVideoProject())
  const proposal: VideoCreativeProposal = { id: 'proposal', kind: 'storyboard', changes: ['morning', 'noon', 'evening'].map(shotId => ({ shotId, action: 'add', name: shotId, storyboard: '', sources: [] })) }
  const first = applyVideoCreativeProposal({ ...original, proposals: [proposal] }, proposal.id, ['evening'])
  const second = applyVideoCreativeProposal(first, proposal.id, ['morning'])
  const third = applyVideoCreativeProposal(second, proposal.id, ['noon'])
  expect(third.shots.map(shot => shot.id)).toEqual(['morning', 'noon', 'evening'])
})
