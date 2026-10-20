import { createFileRoute, Outlet } from '@tanstack/react-router'
import { SidebarProvider } from '@/components/ui/sidebar'
import { SkipToMain } from '@/components/skip-to-main'

export const Route = createFileRoute('/_app')({
  component: () => <SidebarProvider><SkipToMain /><Outlet /></SidebarProvider>,
})
