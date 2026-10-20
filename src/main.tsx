import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { DirectionProvider } from './context/direction-provider'
import { FontProvider } from './context/font-provider'
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
  await initI18n()
  const element = document.getElementById('root')
  if (!element) throw new Error('应用挂载位置不存在')
  createRoot(element).render(
    <StrictMode><QueryClientProvider client={queryClient}><ThemeProvider><FontProvider><DirectionProvider>
      <RouterProvider router={router} />
    </DirectionProvider></FontProvider></ThemeProvider></QueryClientProvider></StrictMode>,
  )
}

void bootstrap()
