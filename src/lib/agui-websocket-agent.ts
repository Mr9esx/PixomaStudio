import {
  AbstractAgent,
  type AgentConfig,
  type BaseEvent,
  type RunAgentInput,
} from '@ag-ui/client'
import { Observable } from 'rxjs'
import type { StudioComposerPart, StudioRun } from '@/lib/api/studio'
import { desktopInvoke } from '@/lib/api/client'
import { listenStudioRun } from '@/lib/desktop-events'

export type StudioRunConfig = {
  modelConfigId: string
  locale: 'zh' | 'en'
  permissionMode: string
  selectedSkillIds: string[]
  selectedAssets: Array<{ assetId: string; assetVersionId: string }>
  messageParts?: StudioComposerPart[]
  expertId?: string
  expertVersion?: string
}

type Config = AgentConfig & {
  url: string
  runConfig: StudioRunConfig
  onEvent?: (event: BaseEvent) => void
}

export class StudioWebSocketAgent extends AbstractAgent {
  private readonly url: string
  private readonly onEvent?: (event: BaseEvent) => void
  private runConfig: StudioRunConfig
  private nextRunConfig?: StudioRunConfig
  private studioRunId?: string
  private stopActiveRun?: () => void

  activeStudioRunId(): string | undefined { return this.studioRunId }

  constructor(config: Config) {
    super(config)
    this.url = config.url
    this.onEvent = config.onEvent
    this.runConfig = config.runConfig
  }

  updateRunConfig(runConfig: StudioRunConfig): void { this.runConfig = runConfig }

  prepareNextRun(runConfig: StudioRunConfig): void { this.nextRunConfig = runConfig }

  run(input: RunAgentInput): Observable<BaseEvent> {
    const runConfig = this.nextRunConfig ?? this.runConfig
    this.nextRunConfig = undefined
    const request = {
      ...input,
      requestId: input.runId,
      forwardedProps: { ...input.forwardedProps, runConfig },
    }
    return new Observable<BaseEvent>((subscriber) => {
      this.studioRunId = undefined
      const controller = new AbortController()
      const stop = () => { controller.abort(); subscriber.complete() }
      this.stopActiveRun = stop
      void (async () => {
        const run = await desktopInvoke<StudioRun>('studio_run', { request })
        this.studioRunId = run.id
        await listenStudioRun(run.id, 0, (event) => {
          const payload = event as unknown as BaseEvent
          this.onEvent?.(payload)
          subscriber.next(payload)
        }, controller.signal)
        subscriber.complete()
      })().catch((error: unknown) => subscriber.error(error))
      return () => {
        if (this.stopActiveRun === stop) this.stopActiveRun = undefined
        controller.abort()
      }
    })
  }

  override abortRun(): void { this.stopActiveRun?.() }

  override clone(): StudioWebSocketAgent {
    return new StudioWebSocketAgent({
      url: this.url, onEvent: this.onEvent, runConfig: this.runConfig,
      agentId: this.agentId, description: this.description,
      threadId: this.threadId, initialMessages: this.messages, initialState: this.state,
    })
  }
}
