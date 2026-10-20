import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { ApiError } from '@/lib/api/client'
import { apiErrorMessage } from '@/lib/api/error-copy'
import { loginStudio, type StudioUser } from '@/lib/api/operations'
import { useTheme } from '@/context/theme-provider'
import { RequiredBadge } from '@/components/required-badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { ThemeSwitcher } from '@/components/kibo-ui/theme-switcher'
import { PasswordInput } from '@/components/password-input'
import { LocaleSwitcher } from '@/features/setup/locale-switcher'
import { StudioAuthShell, type StudioAuthAmbient } from './studio-auth-shell'

export function StudioLoginPage({
  onLogin,
  registrationOpen = false,
}: {
  onLogin: (user: StudioUser) => void
  registrationOpen?: boolean
}) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [remember, setRemember] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const [ambient] = useState<StudioAuthAmbient>(() =>
    Math.random() < 0.5 ? 'dither' : 'terminal'
  )
  const { theme, setTheme } = useTheme()
  const { t } = useTranslation()

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault()
    setPending(true)
    setError(null)
    try {
      const user = await loginStudio(username.trim(), password, remember)
      setPassword('')
      onLogin(user)
    } catch (err) {
      setError(err instanceof ApiError ? apiErrorMessage(err, t) : t('auth.loginFailed'))
    } finally {
      setPending(false)
    }
  }

  return (
    <StudioAuthShell ambient={ambient} logoVariant='brand'>
      <Card className='w-full max-w-sm'>
        <CardHeader>
          <CardTitle>{t('auth.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit}>
            <FieldGroup className='gap-4'>
              <Field>
                <FieldLabel htmlFor='studio-username'>
                  {t('auth.username')}
                  <RequiredBadge />
                </FieldLabel>
                <Input
                  id='studio-username'
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  autoComplete='username'
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor='studio-password'>
                  {t('auth.password')}
                  <RequiredBadge />
                </FieldLabel>
                <PasswordInput
                  id='studio-password'
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete='current-password'
                  required
                />
              </Field>
              <label className='flex cursor-pointer items-center gap-2 text-sm text-muted-foreground'>
                <Checkbox
                  checked={remember}
                  onCheckedChange={(v) => setRemember(v === true)}
                />
                {t('auth.remember')}
              </label>
            </FieldGroup>
            {error ? <p role='alert' className='text-sm text-destructive'>{error}</p> : null}
            <Button type='submit' className='w-full mt-4' disabled={pending}>
              {pending ? t('auth.submitting') : t('auth.submit')}
            </Button>
            {registrationOpen ? (
              <p className='text-center text-sm text-muted-foreground'>
                {t('auth.registrationPrompt')}{' '}
                <Link to='/studio/register' className='text-primary hover:underline'>
                  {t('auth.createAccount')}
                </Link>
              </p>
            ) : null}
          </form>
        </CardContent>
      </Card>
      <div
        className='fixed right-4 bottom-4 z-30 flex items-center gap-2'
        data-testid='login-theme-switcher'
      >
        <LocaleSwitcher />
        <ThemeSwitcher value={theme} onChange={setTheme} />
      </div>
    </StudioAuthShell>
  )
}
