import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'

export function StudioSettingsLayout<Section extends string>({
  title,
  sections,
  section,
  onSectionChange,
  children,
}: {
  title: string
  sections: { id: Section; label: string }[]
  section: Section
  onSectionChange: (section: Section) => void
  children: ReactNode
}) {
  return (
    <section className='flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden'>
      <header data-tauri-drag-region='deep' className='studio-page-header'>
        <h1 className='text-sm font-semibold'>{title}</h1>
      </header>
      <div className='flex min-h-0 flex-1 flex-col min-[920px]:flex-row'>
        <div className='flex flex-wrap gap-1 border-b p-2 min-[920px]:hidden' aria-label={title}>
          {sections.map((item) => (
            <Button
              key={item.id}
              variant={section === item.id ? 'secondary' : 'ghost'}
              size='sm'
              className='min-h-10 shrink-0'
              aria-current={section === item.id ? 'page' : undefined}
              onClick={() => onSectionChange(item.id)}
            >
              {item.label}
            </Button>
          ))}
        </div>
        <nav className='hidden w-44 shrink-0 border-e bg-muted/20 p-3 min-[920px]:block' aria-label={title}>
          <div className='flex flex-col gap-1'>
            {sections.map((item) => (
              <Button
                key={item.id}
                variant={section === item.id ? 'secondary' : 'ghost'}
                size='default'
                className='w-full justify-start'
                aria-current={section === item.id ? 'page' : undefined}
                onClick={() => onSectionChange(item.id)}
              >
                {item.label}
              </Button>
            ))}
          </div>
        </nav>
        <div className='flex min-h-0 min-w-0 flex-1 flex-col'>{children}</div>
      </div>
    </section>
  )
}
