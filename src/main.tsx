import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { DirectionProvider } from './context/direction-provider'
import { ThemeProvider } from './context/theme-provider'
import { initI18n } from './lib/i18n'
import { routeTree } from './routeTree.gen'
import './styles/index.css'

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 10_000 } } })
const router = createRouter({ routeTree, context: { queryClient }, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register { router: typeof router }
}

async function bootstrap() {
  document.addEventListener('contextmenu', (event) => event.preventDefault())
  document.documentElement.dataset.platform = navigator.userAgent.includes('Mac') ? 'macos' : 'other'
  await initI18n()
  const element = document.getElementById('root')
  if (!element) throw new Error('应用挂载位置不存在')
  createRoot(element).render(
    <StrictMode><QueryClientProvider client={queryClient}><ThemeProvider><DirectionProvider>
      <div aria-hidden='true' data-slot='desktop-window-drag-region' data-tauri-drag-region className='fixed inset-x-0 top-0 z-40 h-3 sm:h-4' />
      <RouterProvider router={router} />
    </DirectionProvider></ThemeProvider></QueryClientProvider></StrictMode>,
  )
}

void bootstrap()
