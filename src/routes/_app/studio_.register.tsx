import { createFileRoute, Navigate, redirect } from '@tanstack/react-router'
import { getStudioRegistration } from '@/lib/api/operations'
import { StudioAuthGate } from '@/features/studio/studio-auth'

export const Route = createFileRoute('/_app/studio_/register')({
  beforeLoad: async () => {
    const registration = await getStudioRegistration()
    if (!registration.enabled) throw redirect({ to: '/studio/login' })
  },
  component: () => <StudioAuthGate authPage='register'><Navigate to='/studio' replace /></StudioAuthGate>,
})
