import { describe, expect, it } from 'vitest'
import { adoptVideoMedia, collectVideoMedia, duplicateVideoShot, emptyVideoProject, fitVideoProjectTiming, normalizeVideoProject, previewVideoShot, removeVideoReference, resolveVideoReferenceSource, setVideoReference, quantizeVideoDuration, teaVideoShots, updateVideoProjectSettings, validateVideoProject, videoTimelineFrames, videoGenerationChanged, videoShotLastFrame, videoReferenceSourceChanged, videoReferencePosition } from './video-editor-model'

describe('视频项目', () => {
  it('复制分镜默认保留创作设置，明确选择后复用视频范围，任务独立', () => {
    const media = { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 6 }
    const source = { ...teaVideoShots()[0], plannedDuration: 5, sourceIn: 1, media, workflowId: 'workflow', versionId: 'published', inputs: { prompt: '晨光', duration: 8 }, executionId: 'running', executionInputs: '{}', generationRequest: { id: 'request', workflowId: 'workflow', versionId: 'published', inputs: {} }, versions: [{ id: 'candidate', media, status: 'candidate' as const }] }
    const copy = duplicateVideoShot(source, 'copy')
    expect(copy).toMatchObject({ id: 'copy', plannedDuration: 5, duration: 0, sourceIn: 0, workflowId: 'workflow', versionId: 'published', inputs: source.inputs, versions: [] })
    expect(copy.media).toBeUndefined()
    expect(copy.executionId).toBeUndefined()
    expect(copy.executionInputs).toBeUndefined()
    expect(copy.generationRequest).toBeUndefined()
    const reused = duplicateVideoShot(source, 'reused', true)
    expect(reused).toMatchObject({ media, sourceIn: 1, duration: 3, versions: [] })
    reused.inputs.prompt = '新内容'
    expect(source.inputs.prompt).toBe('晨光')
    expect(() => duplicateVideoShot(source, source.id)).toThrow('标识')
  })
  it('修改规格重新计算分镜帧边界和音频范围，保留原始作品', () => {
    const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1.1 }], audio: [{ id: 'music', assetId: 'asset', versionId: 'version', url: '/audio.wav', start: 0, duration: 1.1, sourceIn: 0, volume: 1, loop: true }] }
    const next = updateVideoProjectSettings(project, { name: '晨光', width: 1920, height: 1080, fps: 24 })
    expect(next).toMatchObject({ name: '晨光', width: 1920, height: 1080, fps: 24 })
    expect(next.shots[0].duration).toBe(26 / 24)
    expect(next.audio?.[0].duration).toBe(26 / 24)
    expect(next.shots[0].script).toBe(project.shots[0].script)
    expect(project.shots[0].duration).toBe(1.1)
    expect(project.audio[0].duration).toBe(1.1)
    expect(() => validateVideoProject(next)).not.toThrow()
    const renamed = updateVideoProjectSettings(project, { name: '茶园', width: 1920, height: 1080, fps: project.fps })
    expect(renamed.shots).toEqual(project.shots)
    expect(renamed.audio).toEqual(project.audio)
  })

  it('降低帧率后每个分镜至少保留一帧，超出素材范围时拒绝修改', () => {
    const project = { ...emptyVideoProject(), shots: [{ ...teaVideoShots()[0], duration: 1 / 30 }] }
    const settings = { name: project.name, width: 1280, height: 720, fps: 1 }
    expect(updateVideoProjectSettings(project, settings).shots[0].duration).toBe(1)
    const withMedia = { ...project, shots: [{ ...project.shots[0], media: { assetId: 'video', versionId: 'version', url: '/video.mp4', duration: 1 / 30 } }] }
    expect(() => updateVideoProjectSettings(withMedia, settings)).toThrow('剪辑范围超出源视频')
    expect(withMedia.shots[0].duration).toBe(1 / 30)
  })

  it('拒绝无法导出的尺寸、帧率及超出数量限制的作品', () => {
    const project = emptyVideoProject()
    for (const width of [15, 721, 4098, NaN]) expect(() => validateVideoProject({ ...project, width })).toThrow()
    for (const fps of [0, 24.5, 61]) expect(() => validateVideoProject({ ...project, fps })).toThrow()
    expect(() => validateVideoProject({ ...project, shots: Array.from({ length: 101 }, (_, index) => ({ ...teaVideoShots()[0], id: `shot-${index}` })) })).toThrow('分镜最多 100 项')
    const audio = { id: 'music', assetId: 'asset', versionId: 'version', url: '/audio.wav', start: 0, duration: 1, sourceIn: 0, volume: 1, loop: true }
    expect(() => validateVideoProject({ ...project, shots: [teaVideoShots()[0]], audio: Array.from({ length: 101 }, (_, index) => ({ ...audio, id: `audio-${index}` })) })).toThrow('音频最多 100 项')
  })

  it('迁移旧作品并保留剧本、字幕、参数和素材引用', () => {
    const legacy = { version: 1 as const, name: '晨光', width: 1280, height: 720, fps: 30, shots: [{ id: 'shot', name: '开场', script: '晨光', subtitle: '清晨', duration: 3, sourceIn: 0, inputs: { prompt: '晨光穿过树林' } }] }
    const next = normalizeVideoProject(legacy)
    expect(next.version).toBe(2)
    expect(next.shots[0]).toEqual({ ...legacy.shots[0], duration: 0, plannedDuration: 3, storyboard: '', contentRevisions: { script: 1, storyboard: 1 }, references: [], volume: 1, versions: [], scriptSources: [{ blockId: 'script-shot', text: '晨光', revision: 1 }] })
    expect(next.script).toEqual({ revision: 1, blocks: [{ id: 'script-shot', text: '晨光', revision: 1 }] })
    expect(next.audio).toEqual([])
    expect(legacy.shots[0]).not.toHaveProperty('references')
    expect(normalizeVideoProject(next)).toEqual(next)
    expect(() => normalizeVideoProject({ ...legacy, version: 99 })).toThrow('视频项目格式无效')
  })

  it('参考按素材版本和用途去重，单个首帧替换并保留其他用途', () => {
    const shot = teaVideoShots()[0]
    const first = { id: 'ref-a', assetId: 'image-a', versionId: 'version-a', url: '/a.png', kind: 'image' as const, role: 'first_frame' as const }
    const next = setVideoReference(shot, first)
    expect(setVideoReference(next, { ...first, id: 'ref-b' }).references).toEqual([first])
    const character = { ...first, id: 'character', role: 'character' as const }
    const withCharacter = setVideoReference(next, character)
    const replacement = { ...first, id: 'ref-c', assetId: 'image-b', versionId: 'version-b' }
    expect(setVideoReference(withCharacter, replacement).references).toEqual([character, replacement])
    expect(() => setVideoReference(shot, { ...first, kind: 'audio' })).toThrow('参考用途与资产类型不匹配')
  })

  it('参考替换与移除同步更新参数，保留用户重新选择的参数', () => {
    const reference = { id: 'first', assetId: 'image-a', versionId: 'v1', url: '/a.png', kind: 'image' as const, role: 'first_frame' as const, inputKey: 'first_frame' }
    const shot = setVideoReference(teaVideoShots()[0], reference)
    expect(shot.inputs.first_frame).toEqual({ asset_id: 'image-a', asset_version_id: 'v1' })
    const replacement = setVideoReference(shot, { ...reference, id: 'next', assetId: 'image-b', versionId: 'v2', inputKey: 'image' })
    expect(replacement.inputs).toEqual({ image: { asset_id: 'image-b', asset_version_id: 'v2' } })
    expect(removeVideoReference(replacement, 'next').inputs).toEqual({})
    const edited = { ...shot, inputs: { first_frame: { asset_id: 'user-image', asset_version_id: 'user-version' } } }
    expect(removeVideoReference(edited, 'first').inputs).toEqual(edited.inputs)
    expect(shot.references).toEqual([reference])
  })

  it('同一参数只绑定一项参考，其他用途继续保留资产', () => {
    const reference = { id: 'character', assetId: 'image-a', versionId: 'v1', url: '/a.png', kind: 'image' as const, role: 'character' as const, inputKey: 'image' }
    const shot = setVideoReference(teaVideoShots()[0], reference)
    const next = setVideoReference(shot, { ...reference, id: 'scene', assetId: 'image-b', role: 'scene' })
    expect(next.references?.[0].inputKey).toBeUndefined()
    expect(next.inputs.image).toEqual({ asset_id: 'image-b', asset_version_id: 'v1' })
    expect(removeVideoReference(next, 'character').inputs).toEqual(next.inputs)
  })

  it('来源更新可以采用新尾帧或继续使用原参考，来源删除后可以保留独立参考', () => {
    const reference = { id: 'first', assetId: 'frame-a', versionId: 'frame-v1', url: '/a.png', kind: 'image' as const, role: 'first_frame' as const, inputKey: 'first_frame', sourceShotId: 'source', sourceMediaVersionId: 'video-v1', confirmedSourceVersionId: 'video-v1' }
    const shot = setVideoReference(teaVideoShots()[0], reference)
    const source = { ...teaVideoShots()[0], id: 'source', media: { assetId: 'video', versionId: 'video-v2', url: '/video.mp4', lastFrame: { assetId: 'frame-b', versionId: 'frame-v2', url: '/b.png' } } }
    const kept = resolveVideoReferenceSource(shot, 'first', source, false, 30)
    expect(kept.references?.[0]).toEqual({ ...reference, confirmedSourceVersionId: 'video-v2', confirmedSourceFrame: videoShotLastFrame(source, 30), confirmedSourceFps: 30 })
    expect(kept.inputs).toEqual(shot.inputs)
    const updated = resolveVideoReferenceSource(shot, 'first', source, true, 30, source.media.lastFrame)
    expect(updated.references?.[0]).toEqual({ ...reference, ...source.media.lastFrame, sourceMediaVersionId: 'video-v2', confirmedSourceVersionId: 'video-v2', sourceFrame: videoShotLastFrame(source, 30), sourceFps: 30, confirmedSourceFrame: videoShotLastFrame(source, 30), confirmedSourceFps: 30 })
    expect(updated.inputs.first_frame).toEqual({ asset_id: 'frame-b', asset_version_id: 'frame-v2' })
    const detached = resolveVideoReferenceSource(shot, 'first', undefined, false, 30)
    expect(detached.references?.[0].sourceShotId).toBeUndefined()
    expect(detached.inputs).toEqual(shot.inputs)
    expect(() => resolveVideoReferenceSource(shot, 'first', undefined, true, 30)).toThrow('来源分镜没有可用的剪辑画面')
    expect(videoReferenceSourceChanged(updated.references![0], source, 30)).toBe(false)
    expect(videoReferenceSourceChanged(updated.references![0], { ...source, duration: 1, sourceIn: 0.5 }, 30)).toBe(true)
    expect(videoShotLastFrame({ ...source, sourceIn: 0.5, duration: 2 }, 30)).toBe(74)
    const clipped = { ...source, sourceIn: 0.5, duration: 2 }
    expect(videoReferencePosition({ sourcePosition: 'first' }, clipped, 30)).toEqual({ frame: 15, fps: 30 })
    const first = { ...updated.references![0], sourcePosition: 'first' as const, confirmedSourceFrame: 15, confirmedSourceFps: 30 }
    expect(videoReferenceSourceChanged(first, clipped, 30)).toBe(false)
    expect(videoReferenceSourceChanged(first, { ...clipped, duration: 1 }, 30)).toBe(false)
    expect(videoReferenceSourceChanged(first, { ...clipped, sourceIn: 1 }, 30)).toBe(true)
    const fixed = { ...first, sourcePosition: 'fixed' as const, sourceFrame: 20, sourceFps: 30, confirmedSourceFrame: 20 }
    expect(videoReferencePosition(fixed, clipped, 24)).toEqual({ frame: 20, fps: 30 })
    expect(videoReferenceSourceChanged(fixed, { ...clipped, sourceIn: 1, duration: 1 }, 24)).toBe(false)
    expect(videoReferenceSourceChanged(fixed, { ...clipped, media: { ...clipped.media, versionId: 'new-version' } }, 30)).toBe(true)
    expect(videoReferenceSourceChanged(kept.references![0], source, 30)).toBe(false)
    expect(videoReferenceSourceChanged(updated.references![0], undefined, 30)).toBe(true)
  })

  it('分镜预览保留相交音频并按源范围正确定位循环声音', () => {
    const shots = teaVideoShots().slice(0, 3)
    const audio = { id: 'music', assetId: 'music', versionId: 'music-v1', url: '/music.wav', start: 1, duration: 7, sourceIn: 2, sourceDuration: 5, volume: .6, loop: true }
    const project = { ...emptyVideoProject(), shots, audio: [audio, { ...audio, id: 'later', start: 7, duration: 2 }] }
    const preview = previewVideoShot(project, shots[1].id)
    expect(preview.shots).toEqual([shots[1]])
    expect(preview.audio).toEqual([{ ...audio, start: 0, duration: 3, playbackOffset: 2 }])
    expect(project.audio[0]).not.toHaveProperty('playbackOffset')
    expect(() => previewVideoShot(project, 'deleted')).toThrow('分镜已被删除')
    expect(previewVideoShot(project, shots[2].id).audio?.[0].playbackOffset).toBe(5)
  })
  it('逐镜计算累计帧数，保持预览范围与分镜边界一致', () => {
    const shots = teaVideoShots().map(shot => ({ ...shot, duration: 3.01 }))
    expect(videoTimelineFrames(shots, 30)).toEqual([0, 90, 180, 270, 360, 450])
    expect(videoTimelineFrames([], 30)).toEqual([0])
    const project = { ...emptyVideoProject(), shots }
    expect(() => validateVideoProject(project)).toThrow('分镜时长需要符合项目帧间隔')
    expect(() => validateVideoProject({ ...project, shots: shots.map(shot => ({ ...shot, duration: quantizeVideoDuration(shot.duration, project.fps) })) })).not.toThrow()
    expect(quantizeVideoDuration(3.01, 30)).toBe(3)
    expect(quantizeVideoDuration(3.06, 30)).toBeCloseTo(92 / 30)
  })
  it('缩短视频时限制音频范围，删除全部分镜时移除音频', () => {
    const audio = { id: 'music', assetId: 'asset', versionId: 'version', url: '/audio.mp3', start: 1, duration: 10, sourceIn: 0, volume: 1, loop: true }
    const project = { ...emptyVideoProject(), shots: teaVideoShots().slice(0, 1), audio: [audio, { ...audio, id: 'voice', start: 5 }] }
    expect(fitVideoProjectTiming(project).audio).toEqual([{ ...audio, duration: 2 }])
    expect(fitVideoProjectTiming({ ...project, shots: [] }).audio).toEqual([])
  })
  it('五个分镜使用十五秒，生成配置独立保存', () => {
    const project = { ...emptyVideoProject(), shots: teaVideoShots() }
    expect(project.shots.reduce((sum, shot) => sum + shot.duration, 0)).toBe(15)
    expect(new Set(project.shots.map(shot => shot.id)).size).toBe(5)
    project.shots[0].inputs.prompt = '晨雾'
    expect(project.shots[1].inputs.prompt).toBeUndefined()
    expect(() => validateVideoProject(project)).not.toThrow()
  })
  it('拒绝无效剪辑范围与视频规格', () => {
    expect(() => validateVideoProject({ ...emptyVideoProject(), width: 0 })).toThrow()
    const project = { ...emptyVideoProject(), shots: teaVideoShots() }
    project.shots[0].sourceIn = -1
    expect(() => validateVideoProject(project)).toThrow()
  })
  it('重新生成保留当前视频，采用候选后保留历史', () => {
    const media = { assetId: 'video-a', versionId: 'version-a', url: '/a.mp4', duration: 5 }
    const next = { assetId: 'video-b', versionId: 'version-b', url: '/b.mp4', duration: 4 }
    const shot = { ...teaVideoShots()[0], media, executionId: 'job', executionInputs: '{}' }
    const collected = collectVideoMedia(shot, 'job', next)
    expect(collected.media).toEqual(media)
    expect(collected.versions?.[0].status).toBe('candidate')
    expect(collectVideoMedia(collected, 'job', next)).toBe(collected)
    const adopted = adoptVideoMedia(collected, next)
    expect(adopted.media).toEqual(next)
    expect(adopted.versions).toEqual([{ id: media.versionId, media, status: 'history', sourceIn: shot.sourceIn, duration: shot.duration }])
    expect(collectVideoMedia(shot, 'old-job', next)).toBe(shot)
  })
  it('首次生成和输入修改后的结果均进入候选，等待明确采用', () => {
    const media = { assetId: 'video', versionId: 'version', url: '/video.mp4' }
    const shot = { ...teaVideoShots()[0], inputs: { prompt: '茶园' }, executionId: 'job', executionInputs: JSON.stringify({ prompt: '茶园' }) }
    expect(collectVideoMedia(shot, 'job', media).media).toBeUndefined()
    expect(collectVideoMedia(shot, 'job', media).versions?.[0]).toMatchObject({ media, status: 'candidate', executionId: 'job' })
    expect(collectVideoMedia({ ...shot, inputs: { prompt: '茶叶' } }, 'job', media).versions?.[0].status).toBe('candidate')
    expect(collectVideoMedia(shot, 'job', { ...media, duration: 2 }).versions?.[0].status).toBe('candidate')
    const first = collectVideoMedia(shot, 'job', media)
    const second = collectVideoMedia(first, 'job', { ...media, versionId: 'second-version' })
    expect(second.versions?.map(item => item.media.versionId)).toEqual(['version', 'second-version'])
    expect(second.media).toBeUndefined()
  })
  it('按独立生成记录收集历史任务结果，并检查目标分镜', () => {
    const shot = { ...teaVideoShots()[0], executionId: 'new-job' }
    const media = { assetId: 'video', versionId: 'version', url: '/video.mp4', generation: { executionId: 'old-job', workflowId: 'workflow', versionId: 'workflow-version', inputs: { prompt: '原始输入' } } }
    const target = { shotId: shot.id, executionId: 'old-job' }
    const collected = collectVideoMedia(shot, target, media)
    expect(collected.executionId).toBe('new-job')
    expect(collected.versions?.[0]).toMatchObject({ executionId: 'old-job', status: 'candidate', media })
    expect(collectVideoMedia(collected, target, media)).toBe(collected)
    expect(() => collectVideoMedia(shot, { ...target, shotId: 'another-shot' }, media)).toThrow('目标分镜')
    expect(() => collectVideoMedia(shot, { ...target, executionId: 'unrelated-job' }, media)).toThrow('生成任务')
  })
  it('采用较短视频需要明确使用范围，历史版本保留原剪辑', () => {
    const previous = { assetId: 'previous', versionId: 'v1', url: '/previous.mp4', duration: 8 }
    const media = { assetId: 'next', versionId: 'v2', url: '/next.mp4', duration: 2 }
    const shot = { ...teaVideoShots()[0], media: previous, sourceIn: 1, duration: 4 }
    expect(() => adoptVideoMedia(shot, media)).toThrow('使用范围')
    const next = adoptVideoMedia(shot, media, { sourceIn: 0, duration: 2 })
    expect(next).toMatchObject({ media, sourceIn: 0, duration: 2 })
    expect(next.versions).toContainEqual({ id: 'v1', media: previous, status: 'history', sourceIn: 1, duration: 4 })
    expect(shot).toMatchObject({ media: previous, sourceIn: 1, duration: 4 })
  })
  it('首次采用使用完整媒体范围，并拒绝未知时长和越界范围', () => {
    const shot = teaVideoShots()[0]
    const media = { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 5 }
    expect(adoptVideoMedia(shot, media)).toMatchObject({ duration: 5, sourceIn: 0 })
    expect(() => adoptVideoMedia(shot, { ...media, duration: undefined })).toThrow('时长')
    for (const range of [{ sourceIn: -1, duration: 2 }, { sourceIn: 4, duration: 2 }, { sourceIn: 0, duration: 0 }, { sourceIn: NaN, duration: 1 }]) {
      expect(() => adoptVideoMedia(shot, media, range)).toThrow('使用范围')
    }
  })
  it('采用视频按其生成快照比较输入，忽略字段顺序、创作文本和最新任务', () => {
    const generation = { executionId: 'old-job', workflowId: 'workflow', versionId: 'published', inputs: { prompt: '茶园', image: { asset_id: 'person', asset_version_id: 'v1' } } }
    const media = { assetId: 'video', versionId: 'v1', url: '/video.mp4', duration: 5, generation }
    const shot = { ...teaVideoShots()[0], media, workflowId: 'workflow', versionId: 'published', inputs: { image: { asset_version_id: 'v1', asset_id: 'person' }, prompt: '茶园' }, executionInputs: '{"prompt":"其他任务"}' }
    expect(videoGenerationChanged(shot)).toBe(false)
    expect(videoGenerationChanged({ ...shot, script: '修改原文', storyboard: '修改描述', sourceIn: 1 })).toBe(false)
    expect(videoGenerationChanged({ ...shot, versionId: 'new-published' })).toBe(true)
    expect(videoGenerationChanged({ ...shot, inputs: { ...shot.inputs, prompt: '茶叶' } })).toBe(true)
    expect(videoGenerationChanged({ ...shot, media: { ...media, generation: undefined } })).toBe(false)
    const collected = collectVideoMedia({ ...shot, executionId: 'job' }, 'job', { ...media, versionId: 'v2' })
    generation.inputs.prompt = '提交后变化'
    expect(collected.versions?.[0].media.generation?.inputs.prompt).toBe('茶园')
  })
  it('拒绝重复分镜、超出时长的音频和无效音量', () => {
    const project = { ...emptyVideoProject(), shots: teaVideoShots() }
    expect(() => validateVideoProject({ ...project, shots: [project.shots[0], project.shots[0]] })).toThrow()
    expect(() => validateVideoProject({ ...project, shots: [{ ...project.shots[0], volume: 3 }] })).toThrow()
    expect(() => validateVideoProject({ ...project, audio: [{ id: 'audio', assetId: 'asset', versionId: 'version', url: '/audio.mp3', start: 15, duration: 1, sourceIn: 0, volume: 1, loop: true }] })).toThrow()
    const audio = { id: 'audio', assetId: 'asset', versionId: 'version', url: '/audio.mp3', start: 0, duration: 10, sourceIn: 1, volume: 1, loop: true, sourceDuration: 3 }
    expect(() => validateVideoProject({ ...project, audio: [audio] })).not.toThrow()
    expect(() => validateVideoProject({ ...project, audio: [{ ...audio, loop: false }] })).toThrow('音频剪辑范围超出素材')
    expect(() => validateVideoProject({ ...project, audio: [{ ...audio, sourceIn: 3 }] })).toThrow('音频剪辑范围超出素材')
  })
})
