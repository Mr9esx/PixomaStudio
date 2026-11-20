export const FPS = 30

export function seedProject() {
  return {
    version: 2, name: '山间茶事', ratio: '16:9', subtitles: true,
    assets: [
      { id: 'ref-1', name: '山间晨雾', kind: 'image', scene: 'mountain' },
      { id: 'ref-2', name: '新茶叶片', kind: 'image', scene: 'tea' },
      { id: 'ref-3', name: '手作茶盏', kind: 'image', scene: 'cup' },
      { id: 'asset-brand', name: '品牌视觉参考', kind: 'image', scene: 'mountain' },
    ],
    shots: [
      { id: 'shot-1', name: '雾起山间', duration: 180, sourceIn: 0, sourceDuration: 600, speed: 1, scene: 'mountain', ready: true, script: '清晨，薄雾漫过山脊。茶园在晨光里慢慢醒来。', storyboard: '远景 · 缓慢推进，雾气沿山谷流动，保持自然光。', subtitle: '把时间，交还给山间。', references: [{ id: 'ref-1', name: '山间晨雾', kind: 'image', scene: 'mountain', role: '场景' }], versions: [] },
      { id: 'shot-2', name: '一叶一晨', duration: 180, sourceIn: 0, sourceDuration: 600, speed: 1, scene: 'tea', ready: true, script: '沿着叶片的纹路，记录一片新茶的清晨。', storyboard: '特写 · 轻微横移，叶片上的露珠清晰可见。', subtitle: '一叶之间，自有天地。', references: [{ id: 'ref-2', name: '新茶叶片', kind: 'image', scene: 'tea', role: '场景' }], versions: [] },
      { id: 'shot-3', name: '茶香入席', duration: 120, sourceIn: 0, sourceDuration: 600, speed: 1, scene: 'cup', ready: true, script: '热水缓缓注入茶盏，水面泛起涟漪。', storyboard: '近景 · 固定镜头，茶汤和蒸汽形成画面中心。', subtitle: '山间茶事，静候一盏。', references: [{ id: 'ref-3', name: '手作茶盏', kind: 'image', scene: 'cup', role: '主体' }], versions: [] },
    ],
  }
}

export function migrateProject(project) {
  if (project.version === 1) {
    const assets = [...(project.assets ?? []), ...project.shots.flatMap(shot => shot.references)]
    return { ...project, version: 2, assets: Array.from(new Map(assets.map(asset => [asset.id, asset])).values()) }
  }
  if (project.version !== 2 || !Array.isArray(project.assets)) throw new Error('项目资产数据格式无效')
  return project
}

export function layoutShots(shots) {
  let from = 0
  return shots.map(shot => {
    const item = { ...shot, from, end: from + shot.duration }
    from = item.end
    return item
  })
}

export function editShot(state, id, changes) {
  const original = state.shots.find(shot => shot.id === id)
  if (!original) throw new Error('分镜不存在')
  const shot = { ...original, ...changes }
  if ((original.ready || original.media) && ['script', 'storyboard', 'references'].some(key => Object.hasOwn(changes, key) && JSON.stringify(changes[key]) !== JSON.stringify(original[key]))) shot.inputsChanged = true
  if (!shot.name.trim()) throw new Error('填写分镜名称')
  if (!Number.isInteger(shot.duration) || shot.duration < 1) throw new Error('分镜至少保留一帧')
  if (!(shot.speed >= 0.25 && shot.speed <= 4)) throw new Error('播放速度超出范围')
  if (shot.sourceIn < 0 || shot.sourceIn + Math.ceil(shot.duration * shot.speed) > shot.sourceDuration) throw new Error('剪辑超出源素材范围')
  return { ...state, shots: state.shots.map(item => item.id === id ? shot : item) }
}

export function moveShot(state, id, targetId) {
  if (id === targetId) return state
  const moving = state.shots.find(shot => shot.id === id)
  if (!moving || !state.shots.some(shot => shot.id === targetId)) throw new Error('分镜不存在')
  const shots = state.shots.filter(shot => shot.id !== id)
  shots.splice(shots.findIndex(shot => shot.id === targetId), 0, moving)
  return { ...state, shots }
}

export function splitShot(state, id, localFrame, newId) {
  const shot = state.shots.find(item => item.id === id)
  if (!shot) throw new Error('分镜不存在')
  if (!Number.isInteger(localFrame) || localFrame <= 0 || localFrame >= shot.duration) throw new Error('播放位置需要位于分镜内部')
  const first = { ...shot, duration: localFrame }
  const second = { ...structuredClone(shot), id: newId, name: `${shot.name} · 后半段`, duration: shot.duration - localFrame, sourceIn: shot.sourceIn + Math.round(localFrame * shot.speed) }
  return { ...state, shots: state.shots.flatMap(item => item.id === id ? [first, second] : [item]) }
}

export function removeShot(state, id) {
  return { ...state, shots: state.shots.filter(shot => shot.id !== id) }
}

export function applyHistory(history, next) {
  return { past: [...history.past, history.present].slice(-50), present: next, future: [] }
}

export function newShot(id) {
  return { id, name: '未命名分镜', duration: 150, sourceIn: 0, sourceDuration: 1800, speed: 1, scene: 'mountain', ready: false, script: '', storyboard: '', subtitle: '', references: [], versions: [] }
}

export function timecode(frame) {
  const seconds = Math.floor(frame / FPS)
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}:${String(frame % FPS).padStart(2, '0')}`
}
