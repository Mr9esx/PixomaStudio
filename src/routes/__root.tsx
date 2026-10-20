import type { QueryClient } from '@tanstack/react-query'
import { createRootRouteWithContext, Outlet } from '@tanstack/react-router'
import { Toaster } from '@/components/ui/sonner'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { DesktopMenuNavigation } from '@/components/desktop-menu-navigation'
import { DesktopIdentityBoundary } from '@/components/desktop-identity-boundary'

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: () => <><DesktopMenuNavigation /><DesktopIdentityBoundary><Outlet /></DesktopIdentityBoundary><Toaster duration={5000} /></>,
  errorComponent: ({ error }) => <ErrorBanner message={error instanceof Error ? error.message : String(error)} />,
})
