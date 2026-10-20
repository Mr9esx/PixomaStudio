import { useState, type FormEvent } from 'react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { ApiError } from '@/lib/api/client'
import { apiErrorMessage } from '@/lib/api/error-copy'
import { registerStudio, type StudioUser } from '@/lib/api/operations'
import { RequiredBadge } from '@/components/required-badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { SecretInput } from '@/components/secret-input'
import { StudioAuthShell } from './studio-auth-shell'

export function StudioRegisterPage({ onRegister }: { onRegister: (user: StudioUser) => void }) {
  const { t } = useTranslation()
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [nickname, setNickname] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    if (username.trim().length < 1) {
      setError(t('auth.usernameRequired'))
      return
    }
    if (password.length < 8) {
      setError(t('settings.passwordTooShort'))
      return
    }
    if (password !== confirm) {
      setError(t('settings.passwordMismatch'))
      return
    }
    setPending(true)
    setError(null)
    try {
      const user = await registerStudio({
        username: username.trim(),
        email: email.trim(),
        nickname: nickname.trim(),
        password,
      })
      setPassword('')
      setConfirm('')
      onRegister(user)
    } catch (err) {
      setError(err instanceof ApiError ? apiErrorMessage(err, t) : t('auth.registerFailed'))
    } finally {
      setPending(false)
    }
  }

  return (
    <StudioAuthShell>
      <Card className='w-full max-w-md'>
        <CardHeader>
          <CardTitle>{t('auth.registerTitle')}</CardTitle>
          <CardDescription>{t('auth.studioRegisterDesc')}</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} data-testid='register-form'>
            <FieldGroup className='gap-3'>
              <Field>
                <FieldLabel htmlFor='studio-register-username'>
                  {t('auth.registerUsername')}
                  <RequiredBadge />
                </FieldLabel>
                <Input
                  id='studio-register-username'
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  autoComplete='off'
                  autoFocus
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor='studio-register-email'>
                  {t('setup.email')}
                </FieldLabel>
                <Input
                  id='studio-register-email'
                  type='email'
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  autoComplete='off'
                />
              </Field>
              <Field>
                <FieldLabel htmlFor='studio-register-nickname'>
                  {t('setup.nickname')}
                </FieldLabel>
                <Input
                  id='studio-register-nickname'
                  value={nickname}
                  onChange={(e) => setNickname(e.target.value)}
                  autoComplete='off'
                />
              </Field>
              <Field>
                <FieldLabel htmlFor='studio-register-password'>
                  {t('auth.registerPassword')}
                  <RequiredBadge />
                </FieldLabel>
                <SecretInput
                  id='studio-register-password'
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  autoComplete='new-password'
                  required
                />
              </Field>
              <Field>
                <FieldLabel htmlFor='studio-register-confirm'>
                  {t('auth.registerConfirm')}
                  <RequiredBadge />
                </FieldLabel>
                <SecretInput
                  id='studio-register-confirm'
                  value={confirm}
                  onChange={(e) => setConfirm(e.target.value)}
                  autoComplete='new-password'
                  required
                />
              </Field>
            </FieldGroup>
            {error ? <p role='alert' className='text-sm text-destructive'>{error}</p> : null}
            <Button
              type='submit'
              disabled={pending}
              data-testid='register-submit'
            >
              {pending ? t('auth.registerSubmitting') : t('auth.registerSubmit')}
            </Button>
            <p className='text-center text-sm text-muted-foreground'>
              {t('auth.hasAccount')}{' '}
              <Link to='/studio/login' className='text-primary hover:underline'>
                {t('auth.goLogin')}
              </Link>
            </p>
          </form>
        </CardContent>
      </Card>
    </StudioAuthShell>
  )
}
