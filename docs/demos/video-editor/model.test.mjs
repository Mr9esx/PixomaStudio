import assert from 'node:assert/strict'
import { test } from 'node:test'
import { seedProject, migrateProject, layoutShots, editShot, moveShot, splitShot, removeShot, applyHistory } from './model.mjs'

test('调整分镜顺序后，全部轨道通过相同分镜 ID 与时间位置关联', () => {
  const state = seedProject()
  const moved = moveShot(state, 'shot-3', 'shot-1')
  assert.deepEqual(moved.shots.map(shot => shot.id), ['shot-3', 'shot-1', 'shot-2'])
  assert.deepEqual(layoutShots(moved.shots).map(shot => shot.from), [0, 120, 300])
  assert.equal(moved.shots[0].script, state.shots[2].script)
})

test('裁剪不能超出源素材，分割保留完整时间范围', () => {
  const state = seedProject()
  assert.throws(() => editShot(state, 'shot-1', { duration: 800 }), /源素材/)
  const divided = splitShot(state, 'shot-1', 90, 'shot-new')
  assert.equal(divided.shots.length, 4)
  assert.equal(divided.shots[0].duration, 90)
  assert.equal(divided.shots[1].sourceIn, 90)
  assert.equal(divided.shots[1].duration, 90)
  assert.equal(layoutShots(divided.shots).at(-1).end, 480)
  assert.throws(() => splitShot(state, 'shot-1', 0, 'invalid'), /分镜内部/)
})

test('删除与撤销保留源数据，不改变发送时保存的对话引用', () => {
  const state = seedProject()
  const scope = ['shot-2']
  const next = removeShot(state, 'shot-2')
  const history = applyHistory({ past: [], present: state, future: [] }, next)
  assert.equal(history.present.shots.length, 2)
  assert.equal(history.past[0].shots.length, 3)
  assert.deepEqual(scope, ['shot-2'])
  assert.equal(state.shots.length, 3)
})

test('项目资产库包含未引用资产，删除分镜不会移除项目资产', () => {
  const state = seedProject()
  assert.ok(Array.isArray(state.assets))
  const referenced = new Set(state.shots.flatMap(shot => shot.references.map(reference => reference.id)))
  assert.ok(state.assets.some(asset => !referenced.has(asset.id)))
  const next = removeShot(state, 'shot-2')
  assert.deepEqual(next.assets, state.assets)
})

test('读取已有项目时保留资产和分镜引用，拒绝未知数据版本', () => {
  const current = seedProject()
  const legacy = { ...current, version: 1, assets: [current.assets[3]] }
  const migrated = migrateProject(legacy)
  assert.equal(migrated.version, 2)
  assert.deepEqual(new Set(migrated.assets.map(asset => asset.id)), new Set(current.assets.map(asset => asset.id)))
  assert.deepEqual(migrated.shots, current.shots)
  assert.equal(migrateProject(current), current)
  assert.throws(() => migrateProject({ ...current, version: 99 }), /数据格式/)
})
