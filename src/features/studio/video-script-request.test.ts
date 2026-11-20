import { expect, it } from 'vitest'
import { updateVideoScriptRequests, videoScriptRequestStatus, type VideoScriptRequest } from './video-script-request'

it('分别保留排队请求，提交结果只关联对应会话和请求', () => {
  const first: VideoScriptRequest = { id: 'first', sessionId: 'video', label: '整片剧本', status: 'queued' }
  const second: VideoScriptRequest = { ...first, id: 'second', label: '分镜二' }
  let requests = updateVideoScriptRequests([first], second)
  requests = updateVideoScriptRequests(requests, { ...first, status: 'submitted', runId: 'run-first', messageId: 'message-first' })
  expect(requests[0].messageId).toBe('message-first')
  expect(requests[1]).toEqual(second)
  requests = updateVideoScriptRequests(requests, { ...second, status: 'cancelled' })
  expect(videoScriptRequestStatus(requests[1])).toBe('已取消')
  expect(requests[0].runId).toBe('run-first')
  expect(updateVideoScriptRequests(requests, { ...first, sessionId: 'another' })).toHaveLength(3)
  expect(videoScriptRequestStatus(requests[0], { status: 'waiting_clarification' })).toBe('等待补充信息')
  expect(videoScriptRequestStatus(requests[0], { status: 'succeeded' })).toBe('对话已完成')
})
