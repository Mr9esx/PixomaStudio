import type { VideoProject } from './video-editor-model'

export type VideoEditConflict = { object: string; field: string; local: unknown; remote: unknown }
const equal = (left: unknown, right: unknown) => JSON.stringify(left) === JSON.stringify(right)

export function mergeVideoProjects(base: VideoProject, local: VideoProject, remote: VideoProject, preference: 'local' | 'remote' = 'local') {
  const conflicts: VideoEditConflict[] = []
  function field(object: string, key: string, before: unknown, current: unknown, incoming: unknown): unknown {
    if (equal(current, incoming) || equal(incoming, before)) return current
    if (equal(current, before)) return incoming
    conflicts.push({ object, field: key, local: current, remote: incoming })
    return preference === 'local' ? current : incoming
  }
  function entity<T extends object>(name: string, before: T, current: T, incoming: T): T {
    const keys = new Set([...Object.keys(before), ...Object.keys(current), ...Object.keys(incoming)])
    return Object.fromEntries([...keys].flatMap(key => {
      if (key === 'contentRevisions' && before[key as keyof T] && current[key as keyof T] && incoming[key as keyof T]) return [[key, entity(name, before[key as keyof T] as object, current[key as keyof T] as object, incoming[key as keyof T] as object)]]
      const value = field(name, key, before[key as keyof T], current[key as keyof T], incoming[key as keyof T])
      return value === undefined ? [] : [[key, value]]
    })) as T
  }
  function objects<T extends { id: string }>(name: string, before: T[], current: T[], incoming: T[]): T[] {
    const ids = (values: T[]) => values.map(value => value.id)
    const retained = new Set(before.filter(item => current.some(value => value.id === item.id) && incoming.some(value => value.id === item.id)).map(item => item.id))
    const order = (values: T[]) => ids(values).filter(id => retained.has(id))
    const chosenOrder = field(name, 'order', order(before), order(current), order(incoming)) as string[]
    const all = new Map<string, T>()
    for (const id of new Set([...ids(current), ...ids(incoming), ...ids(before)])) {
      const original = before.find(item => item.id === id), ours = current.find(item => item.id === id), theirs = incoming.find(item => item.id === id)
      const result = original && ours && theirs ? entity(id, original, ours, theirs) : field(id, 'object', original, ours, theirs) as T | undefined
      if (result) all.set(id, result)
    }
    const preferred = equal(chosenOrder, order(incoming)) ? incoming : current
    const outputIds = chosenOrder.filter(id => all.has(id))
    for (const source of [preferred, current, incoming]) {
      const sourceIds = ids(source)
      for (const [index, id] of sourceIds.entries()) {
        if (!all.has(id) || outputIds.includes(id)) continue
        const following = sourceIds.slice(index + 1).find(id => outputIds.includes(id))
        const previous = sourceIds.slice(0, index).reverse().find(id => outputIds.includes(id))
        outputIds.splice(following ? outputIds.indexOf(following) : previous ? outputIds.indexOf(previous) + 1 : outputIds.length, 0, id)
      }
    }
    return outputIds.map(id => all.get(id)!)
  }
  const { shots: baseShots, audio: baseAudio = [], script: baseScript, proposals: baseProposals = [], ...baseFields } = base
  const { shots: localShots, audio: localAudio = [], script: localScript, proposals: localProposals = [], ...localFields } = local
  const { shots: remoteShots, audio: remoteAudio = [], script: remoteScript, proposals: remoteProposals = [], ...remoteFields } = remote
  const project: VideoProject = { ...entity('project', baseFields, localFields, remoteFields), shots: objects('shots', baseShots, localShots, remoteShots), audio: objects('audio', baseAudio, localAudio, remoteAudio) }
  if (baseScript && localScript && remoteScript) {
    const conflictCount = conflicts.length
    const blocks = objects('script', baseScript.blocks, localScript.blocks, remoteScript.blocks)
    const originalIds = baseScript.blocks.map(block => block.id)
    const structural = !equal(originalIds, localScript.blocks.map(block => block.id)) || !equal(originalIds, remoteScript.blocks.map(block => block.id))
    if (structural && conflicts.length > conflictCount) {
      conflicts.splice(conflictCount)
      conflicts.push({ object: 'script', field: 'structure', local: localScript, remote: remoteScript })
      project.script = structuredClone(preference === 'local' ? localScript : remoteScript)
    } else {
      const revision = equal(blocks, localScript.blocks) ? localScript.revision : equal(blocks, remoteScript.blocks) ? remoteScript.revision : Math.max(localScript.revision, remoteScript.revision) + 1
      project.script = { revision, blocks }
    }
  } else {
    const script = field('project', 'script', baseScript, localScript, remoteScript) as VideoProject['script']
    if (script) project.script = script
  }
  if (base.proposals || local.proposals || remote.proposals) project.proposals = objects('proposals', baseProposals, localProposals, remoteProposals)
  return { project, conflicts }
}
