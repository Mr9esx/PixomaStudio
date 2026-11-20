import { describe, expect, it } from 'vitest'
import { emptyVideoProject, newVideoShot, teaVideoShots } from './video-editor-model'
import { restoreVideoEditorView, selectedVideoAsset, videoAssetPreviewTime, videoPreviewFrame } from './video-editor-view'
import type { StudioAsset } from '@/lib/api/studio'
import { createVideoStructureDraft, videoStructureDraftKey } from './video-structure-draft'
import { applyVideoShotStructure } from './video-shot-structure'

describe('编辑器预览状态', () => {
  const project = { ...emptyVideoProject(), shots: teaVideoShots().slice(0, 2) }

  it('恢复未采用的分镜结构草稿，保留空白输入、原文与参考选择并检查来源变化', () => {
    const source = { ...project, shots: [{ ...project.shots[0], references: [{ id: 'frame', assetId: 'frame', versionId: 'frame-version', url: '/frame.png', kind: 'image' as const, role: 'first_frame' as const }] }, project.shots[1]] }
    const draft = createVideoStructureDraft(source, 'split', source.shots[0].id)
    draft.drafts![0].name = ''
    draft.drafts![1].storyboard = '人物走进茶园，镜头缓慢前进'
    draft.drafts![0].references = []
    expect(draft.referenceChoices).toHaveLength(1)
    const key = videoStructureDraftKey(draft.request.mode, draft.request.shotId)
    const saved = JSON.parse(JSON.stringify({ structureDrafts: { [key]: draft } }))
    const changed = { ...source, shots: [{ ...source.shots[0], storyboard: '新的画面' }, source.shots[1]] }
    const restored = restoreVideoEditorView(changed, saved).structureDrafts[key]
    expect(restored).toEqual(draft)
    expect(() => applyVideoShotStructure(changed, [restored.request.project.shots[0]], restored.drafts!)).toThrow('来源分镜已修改')
    expect(restoreVideoEditorView({ ...project, shots: [] }, saved).structureDrafts[key]).toEqual(draft)
    expect(restoreVideoEditorView(project, {}).structureDrafts).toEqual({})
    expect(project.shots[0].name).not.toBe('')
  })

  it('分别保存多个结构草稿的合并范围、生成设置来源和原文修订选择', () => {
    const merged = createVideoStructureDraft(project, 'merge', project.shots[1].id)
    merged.selectedIds = [project.shots[1].id]
    merged.settingsId = project.shots[1].id
    const split = createVideoStructureDraft(project, 'split', project.shots[0].id)
    const drafts = { [videoStructureDraftKey('merge', project.shots[1].id)]: merged, [videoStructureDraftKey('split', project.shots[0].id)]: split }
    expect(restoreVideoEditorView(project, JSON.parse(JSON.stringify({ structureDrafts: drafts }))).structureDrafts).toEqual(drafts)
  })

  it('恢复多选范围，移除重复和已删除的分镜标识', () => {
    const ids = project.shots.map(shot => shot.id)
    expect(restoreVideoEditorView(project, { batchSelection: [ids[1], ids[0], ids[1], 'removed'] }).batchSelection).toEqual([ids[1], ids[0]])
    expect(restoreVideoEditorView(project, {}).batchSelection).toEqual([])
  })

  it('未定作品按当前分镜恢复局部位置，时间确定后转换为整片位置', () => {
    const first = newVideoShot('开场')
    const second = { ...newVideoShot('结尾'), plannedDuration: 3 }
    const incomplete = { ...emptyVideoProject(), shots: [first, second] }
    const saved = { selectedId: second.id, playheadShotId: second.id, playheadFrame: 45 }
    expect(restoreVideoEditorView(incomplete, saved)).toMatchObject({ frame: 45, playheadFrame: 45, previewShotId: second.id })
    expect(restoreVideoEditorView(incomplete, { ...saved, selectedId: first.id })).toMatchObject({ frame: 0 })
    const complete = { ...incomplete, shots: [{ ...first, plannedDuration: 2 }, second] }
    expect(restoreVideoEditorView(complete, saved)).toMatchObject({ frame: 105, playheadFrame: 105 })
  })

  it('保留目标已删除的时间轴文字草稿与编辑基线', () => {
    const textDraft = { id: project.shots[0].id, name: project.shots[0].name, key: 'script', value: '尚未保存的茶园故事', original: project.shots[0].script }
    const saved = restoreVideoEditorView(project, { textDraft })
    const restored = restoreVideoEditorView({ ...project, shots: [] }, JSON.parse(JSON.stringify(saved)))
    expect(restored.textDraft).toEqual(textDraft)
  })

  it('保存和恢复尚未发送的创作想法', () => {
    const ideas = '清晨茶园\n用晨光和茶杯串联故事'
    const view = restoreVideoEditorView(project, { scriptIdeas: ideas })
    expect(restoreVideoEditorView(project, JSON.parse(JSON.stringify(view))).scriptIdeas).toBe(ideas)
    expect(restoreVideoEditorView(project, {}).scriptIdeas).toBe('')
  })

  it('首次进入显示预览，重新打开保留资产 Tab 和浏览位置', () => {
    expect(restoreVideoEditorView(project, {}).library).toBe(false)
    expect(restoreVideoEditorView(project, { library: true, assetScrollTop: 140 })).toMatchObject({ library: true, assetScrollTop: 140 })
  })

  it('按帧定位时间并限制在有效播放范围', () => {
    expect(videoPreviewFrame(1.02, 30, 180)).toBe(31)
    expect(videoPreviewFrame(-1, 30, 180)).toBe(0)
    expect(videoPreviewFrame(6, 30, 180)).toBe(179)
    expect(videoPreviewFrame(1, 30, 0)).toBe(0)
    expect(() => videoPreviewFrame(NaN, 30, 180)).toThrow('播放时间无效')
  })

  it('恢复分镜预览，将整片帧位置转换为分镜内位置', () => {
    const view = restoreVideoEditorView(project, { selectedId: project.shots[0].id, previewShotId: project.shots[1].id, loop: true, playheadFrame: 105, monitorVolume: .4 })
    expect(view.selectedId).toBe(project.shots[0].id)
    expect(view.previewShotId).toBe(project.shots[1].id)
    expect(view.frame).toBe(15)
    expect(view.monitorVolume).toBe(.4)
    expect(project.shots[0].volume).toBe(1)
  })

  it('恢复资产预览时保留固定版本和素材时间，分镜播放位置独立保存', () => {
    const assetPreview = { assetId: 'asset', versionId: 'version-one', time: 12.5 }
    const view = restoreVideoEditorView(project, { assetPreview, previewShotId: project.shots[1].id, loop: true, playheadFrame: 105 })
    expect(view.assetPreview).toEqual(assetPreview)
    expect(view.frame).toBe(15)
    expect(view.previewShotId).toBe(project.shots[1].id)
    expect(view.loop).toBe(true)
    expect(restoreVideoEditorView(project, {}).assetPreview).toBeUndefined()
  })

  it('删除分镜不会清除资产预览，拒绝不完整版本引用与无效素材时间', () => {
    const assetPreview = { assetId: 'asset', versionId: 'version-one', time: 3 }
    expect(restoreVideoEditorView(emptyVideoProject(), { assetPreview, previewShotId: 'removed', loop: true })).toMatchObject({ assetPreview, loop: false, frame: 0 })
    expect(() => restoreVideoEditorView(project, { assetPreview: { ...assetPreview, versionId: '' } })).toThrow()
    expect(() => restoreVideoEditorView(project, { assetPreview: { ...assetPreview, time: -1 } })).toThrow()
    expect(() => restoreVideoEditorView(project, { assetPreview: { ...assetPreview, time: Infinity } })).toThrow()
  })

  it('素材时间按实际时长恢复，时间到达片尾时保留片尾位置', () => {
    expect(videoAssetPreviewTime(2.25, 6)).toBe(2.25)
    expect(videoAssetPreviewTime(12, 6)).toBe(6)
    expect(videoAssetPreviewTime(3, 0)).toBe(0)
    expect(() => videoAssetPreviewTime(-1, 6)).toThrow('素材预览时间无效')
    expect(() => videoAssetPreviewTime(1, NaN)).toThrow('素材预览时间无效')
  })

  it('已删除的选择和循环对象不会阻止恢复，时间限制在现有作品内', () => {
    const view = restoreVideoEditorView(project, { selectedId: 'removed', selectedAudioId: 'removed-audio', tab: 'audio', previewShotId: 'removed', loop: true, playheadFrame: 300 })
    expect(view.selectedId).toBe(project.shots[0].id)
    expect(view.selectedAudioId).toBe('')
    expect(view.tab).toBe('shot')
    expect(view.loop).toBe(false)
    expect(view.frame).toBe(179)
    const empty = restoreVideoEditorView(emptyVideoProject(), { inspector: true, loop: true })
    expect(empty.inspector).toBe(false)
    expect(empty.frame).toBe(0)
  })

  it('拒绝无效状态版本和超出范围的监听音量', () => {
    expect(() => restoreVideoEditorView(project, { version: 2 })).toThrow()
    expect(() => restoreVideoEditorView(project, { monitorVolume: 2 })).toThrow()
  })

  it('恢复资产筛选和两处滚动位置，旧状态使用默认浏览设置', () => {
    const browsing = { search: '晨光', assetKind: 'video', assetView: 'list', assetScrollTop: 240, scrollLeft: 960, scrollTop: 84 }
    expect(restoreVideoEditorView(project, browsing)).toMatchObject(browsing)
    expect(restoreVideoEditorView(project, {})).toMatchObject({ search: '', assetKind: 'all', assetView: 'grid', assetScrollTop: 0, scrollLeft: 0, scrollTop: 0 })
    expect(() => restoreVideoEditorView(project, { assetScrollTop: -1 })).toThrow()
    expect(() => restoreVideoEditorView(project, { scrollLeft: Infinity })).toThrow()
    expect(() => restoreVideoEditorView(project, { assetKind: 'unknown' })).toThrow()
  })

  it('恢复轨道显示设置并保留收起前的高度，拒绝无效高度', () => {
    const display = { trackHeights: { script: 96, video: 64 }, collapsedTracks: ['script'] }
    expect(restoreVideoEditorView(project, display)).toMatchObject(display)
    expect(restoreVideoEditorView(project, {})).toMatchObject({ trackHeights: {}, collapsedTracks: [] })
    expect(() => restoreVideoEditorView(project, { trackHeights: { script: 0 } })).toThrow()
    expect(() => restoreVideoEditorView(project, { trackHeights: { script: 161 } })).toThrow()
  })

  it('未提交的数值输入与错误按对象恢复，作品数值保持不变', () => {
    const key = `shot:${project.shots[0].id}:duration`
    const numberDrafts = { [key]: { text: '-', baseline: 3, error: '填写有效数字' } }
    expect(restoreVideoEditorView(project, { numberDrafts })).toMatchObject({ numberDrafts })
    expect(restoreVideoEditorView(project, {})).toMatchObject({ numberDrafts: {} })
    expect(project.shots[0].duration).toBe(3)
  })

  it('名称草稿独立恢复，空白输入不会破坏作品名称', () => {
    const shot = project.shots[0]
    const nameDrafts = { [shot.id]: { text: ' ', baseline: shot.name, error: '填写分镜名称' } }
    expect(restoreVideoEditorView(project, { nameDrafts })).toMatchObject({ nameDrafts })
    expect(restoreVideoEditorView(project, {})).toMatchObject({ nameDrafts: {} })
    expect(project.shots[0].name.trim()).not.toBe('')
  })

  it('待采用提示词保留目标版本和基线，恢复不会修改分镜参数', () => {
    const before = structuredClone(project)
    const promptDrafts = { draft: { shotId: project.shots[0].id, workflowId: 'workflow', versionId: 'version', inputKey: 'prompt', sourceScript: project.shots[0].script, baseline: '原提示词', text: '编辑中的提示词' } }
    expect(restoreVideoEditorView(project, JSON.parse(JSON.stringify({ promptDrafts })))).toMatchObject({ promptDrafts })
    expect(restoreVideoEditorView(project, {}).promptDrafts).toEqual({})
    expect(project).toEqual(before)
  })

  it('保留选定资产版本，当前版本更新和版本缺失均不改变选择', () => {
    const first = { id: 'version-one', version: 1, mime_type: 'video/mp4', size_bytes: 100, content_url: 'pixoma-media://localhost/asset/version-one', created_at: '2026-11-01T08:00:00Z' }
    const asset: StudioAsset = { id: 'asset', name: '晨光', kind: 'video', origin: 'user', current_version: 2, versions: [first, { ...first, id: 'version-two', version: 2 }], created_at: first.created_at, updated_at: first.created_at }
    expect(selectedVideoAsset(asset)?.current_version).toBe(2)
    expect(selectedVideoAsset(asset, first.id)?.current_version).toBe(1)
    expect(asset.current_version).toBe(2)
    expect(selectedVideoAsset(asset, 'removed')).toBeUndefined()
    const view = restoreVideoEditorView(project, { assetVersions: { asset: first.id } })
    expect(view.assetVersions).toEqual({ asset: first.id })
    expect(selectedVideoAsset(asset, view.assetVersions.asset)?.versions.find(version => version.version === 1)?.id).toBe(first.id)
  })
})
