import { createFileRoute, Navigate } from '@tanstack/react-router'
import { getStudioRegistration } from '@/lib/api/operations'
import { StudioAuthGate } from '@/features/studio/studio-auth'

export const Route = createFileRoute('/_app/studio_/login')({
  beforeLoad: () => getStudioRegistration(),
  component: StudioLoginRoute,
})

function StudioLoginRoute() {
  const { enabled } = Route.useRouteContext()
  return <StudioAuthGate authPage='login' registrationOpen={enabled}><Navigate to='/studio' replace /></StudioAuthGate>
}
