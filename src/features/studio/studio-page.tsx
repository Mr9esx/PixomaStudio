import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export function StudioPage({ className, ...props }: ComponentProps<'main'>) {
  return (
    <main
      id='main-content'
      data-slot='studio-page'
      className={cn('my-3 mr-3 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border bg-card sm:my-4 sm:mr-4 max-lg:ml-3 sm:max-lg:ml-4', className)}
      {...props}
    />
  )
}
