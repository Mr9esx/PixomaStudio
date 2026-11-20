import assert from 'node:assert/strict'
import { test } from 'node:test'
import { seedProject, editShot, newShot } from './model.mjs'
import { shotLifecycle, projectLifecycle, previewShot, nextPreviewStatus } from './lifecycle.mjs'

test('已有视频显示重新生成，空分镜先填写资料', () => {
  assert.equal(shotLifecycle(seedProject().shots[0]).primary.label, '重新生成')
  const empty = shotLifecycle(newShot('empty'))
  assert.equal(empty.status, 'empty')
  assert.equal(empty.primary.label, '完善分镜')
  const prepared = { ...newShot('prepared'), script: '远景，山间晨雾。' }
  assert.equal(shotLifecycle(prepared).primary.label, '生成视频')
})

test('生成活动阶段禁止重复生成，重新生成时仍可播放和导出当前视频', () => {
  const original = seedProject().shots[0]
  for (const status of ['queued', 'running', 'collecting', 'cancelling', 'submission_unknown']) {
    const state = shotLifecycle(previewShot(original, status))
    assert.equal(state.canGenerate, false)
    assert.equal(state.hasVideo, true)
    assert.equal(state.canExport, true)
  }
  assert.equal(shotLifecycle(previewShot(original, 'submission_unknown')).primary.label, '查询状态')
  const firstGeneration = shotLifecycle(previewShot(original, 'running', false))
  assert.equal(firstGeneration.hasVideo, false)
  assert.equal(firstGeneration.canExport, false)
})

test('重试、候选采用和首尾帧处理分别提供正确动作', () => {
  const original = seedProject().shots[0]
  assert.equal(shotLifecycle(previewShot(original, 'failed')).primary.label, '重试生成')
  assert.equal(shotLifecycle(previewShot(original, 'candidate')).primary.label, '查看新版本')
  assert.equal(shotLifecycle(previewShot(original, 'frames_failed')).frameAction, '重试提取')
  assert.equal(shotLifecycle(previewShot(original, 'frames_processing')).canUseTailFrame, false)
  assert.equal(shotLifecycle({ ...original, mediaMissing: true, frames: { status: 'ready' } }).canUseTailFrame, false)
  assert.equal(nextPreviewStatus('queued'), 'running')
  assert.equal(nextPreviewStatus('running'), 'collecting')
  assert.equal(nextPreviewStatus('collecting'), 'candidate')
  assert.equal(nextPreviewStatus('collecting', false), 'frames_processing')
  assert.throws(() => nextPreviewStatus('ready'), /后续状态/)
})

test('修改生成资料保留当前视频，并标记资料变化', () => {
  const original = seedProject()
  const updated = editShot(original, 'shot-1', { script: '山间晨雾，增加鸟鸣。' })
  assert.equal(shotLifecycle(updated.shots[0]).status, 'stale')
  assert.equal(updated.shots[0].ready, true)
  assert.equal(shotLifecycle(editShot(original, 'shot-1', { name: '新名称' }).shots[0]).status, 'ready')
})

test('作品导出检查使用采用视频与保存状态，逐项定位缺失分镜', () => {
  const original = seedProject()
  assert.equal(projectLifecycle(original, '已保存').canExport, true)
  assert.equal(projectLifecycle(original, '保存失败').canExport, false)
  const framesFailed = { ...original, shots: [previewShot(original.shots[0], 'frames_failed')] }
  assert.equal(projectLifecycle(framesFailed, '已保存').canExport, true)
  assert.equal(projectLifecycle(framesFailed, '已保存').tone, 'warn')
  const missing = { ...original, shots: [...original.shots, newShot('empty')] }
  assert.deepEqual(projectLifecycle(missing, '已保存').missingShotIds, ['empty'])
  assert.equal(projectLifecycle({ ...original, shots: [] }, '已保存').status, 'empty')
})
