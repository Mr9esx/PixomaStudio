import { expect, it } from 'vitest'
import { metricDevices } from './observation'

it('使用设备标识区分相同型号的 GPU', () => {
  const devices = metricDevices([{ collected_at: '2026-11-02T00:00:00Z', gpus: [{ id: '0', name: 'RTX 4090' }, { id: '1', name: 'RTX 4090' }] }])
  expect(devices).toEqual([{ id: '0', name: 'RTX 4090' }, { id: '1', name: 'RTX 4090' }])
})
