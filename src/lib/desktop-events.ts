import { Channel } from '@tauri-apps/api/core'
import { desktopInvoke } from '@/lib/api/client'

export type StudioEvent = { type: string; sequence?: number; [key: string]: unknown }

export async function listenStudioRun(
  runId: string,
  afterSequence: number,
  onEvent: (event: StudioEvent) => void,
  signal: AbortSignal
): Promise<void> {
  if (signal.aborted) return
  const subscriptionId = crypto.randomUUID()
  let ready = false
  let resolveReady!: () => void
  const readyPromise = new Promise<void>((resolve) => { resolveReady = resolve })
  let resolveFinished!: () => void
  const finished = new Promise<void>((resolve) => { resolveFinished = resolve })
  let abort!: () => void
  const aborted = new Promise<void>((resolve) => { abort = resolve })
  signal.addEventListener('abort', abort, { once: true })
  const channel = new Channel<StudioEvent>((event) => {
    if (event.type === 'SUBSCRIPTION_READY') {
      ready = true
      resolveReady()
    } else if (event.type === 'SUBSCRIPTION_FINISHED') {
      resolveFinished()
    } else if (!signal.aborted) {
      onEvent(event)
    }
  })
  const subscription = desktopInvoke<void>('studio_subscribe', {
    subscriptionId, runId, afterSequence, onEvent: channel,
  }).then(() => finished)
  try {
    await Promise.race([subscription, aborted])
  } finally {
    signal.removeEventListener('abort', abort)
    if (signal.aborted) {
      await Promise.race([readyPromise, subscription])
      if (ready) await desktopInvoke<void>('studio_unsubscribe', { subscriptionId })
      await subscription
    }
  }
}
