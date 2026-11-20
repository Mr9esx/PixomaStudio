import type { QueryClient } from '@tanstack/react-query'
import { createRootRouteWithContext, Outlet } from '@tanstack/react-router'
import { Toaster } from '@/components/ui/sonner'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { DesktopMenuNavigation } from '@/components/desktop-menu-navigation'

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: () => <><DesktopMenuNavigation /><Outlet /><Toaster duration={5000} /></>,
  errorComponent: ({ error }) => <ErrorBanner message={error instanceof Error ? error.message : String(error)} />,
})
