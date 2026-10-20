import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { ExpertEditor } from '@/features/agent-assets/expert-editor'
import { StudioAuthGate } from '@/features/studio/studio-auth'

export const Route = createFileRoute('/_app/studio_/expert-editor/$expertId')({ component: ExpertEditorPage })

function ExpertEditorPage() {
  const { expertId } = Route.useParams()
  const navigate = useNavigate()
  return <StudioAuthGate><main id='main-content' data-layout='fixed' className='flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border bg-card'>
    <ExpertEditor scope='studio' id={expertId === 'new' ? undefined : expertId} onClose={() => void navigate({ to: '/studio/experts' })} />
  </main></StudioAuthGate>
}
