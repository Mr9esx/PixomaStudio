import { createFileRoute } from '@tanstack/react-router'
import { StudioWorkspace } from '@/features/studio/studio-workspace'
import { StudioAuthGate } from '@/features/studio/studio-auth'

export const Route = createFileRoute('/_app/studio')({
  component: () => <StudioAuthGate><StudioWorkspace /></StudioAuthGate>,
})
