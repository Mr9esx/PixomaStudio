import { useTranslation } from 'react-i18next'
import { StudioSessionManagement } from './studio-session-management'
import { StudioSettingsLayout } from './studio-settings-layout'
import { useLocation, useNavigate } from '@tanstack/react-router'
import { StudioAccountSettings } from './studio-auth'

export function StudioPersonalSettings({
  onSessionsCleared,
}: {
  onSessionsCleared: () => void
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const pathname = useLocation({ select: location => location.pathname })
  const section = pathname.endsWith('/account') ? 'account' : 'sessions'
  return (
    <main
      id='main-content'
      className='flex min-h-0 min-w-0 flex-1 py-3 pr-3 sm:py-4 sm:pr-4'
    >
      <StudioSettingsLayout
        title={t('studioOperations.settings')}
        headerClassName='pl-16 lg:pl-5'
        sections={[{ id: 'sessions', label: t('studioOperations.sessions') }, { id: 'account', label: '账号设置' }]}
        section={section}
        onSectionChange={value => void navigate({ to: '/studio/settings/$section', params: { section: value } })}
      >
        {section === 'account' ? <StudioAccountSettings /> : <StudioSessionManagement onSessionsCleared={onSessionsCleared} />}
      </StudioSettingsLayout>
    </main>
  )
}
