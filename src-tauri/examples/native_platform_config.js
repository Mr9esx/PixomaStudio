import { emit } from '@tauri-apps/api/event'
import { getStudioRegistration } from '../../src/lib/api/operations'

void (async () => {
  try {
    const registration = await getStudioRegistration()
    if (typeof registration.enabled !== 'boolean') throw new Error('平台注册设置格式无效')
    await emit('studio:configuration-result', { success: true, stage: 'registration' })
  } catch (error) {
    await emit('studio:configuration-result', { success: false, stage: 'request_registration', message: String(error), location: location.href })
  }
})()
