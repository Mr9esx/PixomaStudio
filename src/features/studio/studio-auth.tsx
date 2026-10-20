import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ApiError } from '@/lib/api/client'
import { changeStudioPassword, getStudioIdentity, getStudioRegistration, logoutStudio, updateStudioProfile, type StudioProfile, type StudioUser } from '@/lib/api/operations'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { SecretInput } from '@/components/secret-input'
import { toast } from 'sonner'
import { StudioLoginPage } from './studio-login-page'
import { StudioRegisterPage } from './studio-register-page'
import { replaceStudioIdentity } from '@/lib/identity-cache'

const studioIdentityKey = ['studio', 'identity']

export function StudioAuthGate({ children, authPage, registrationOpen = false }: { children: ReactNode; authPage?: 'login' | 'register'; registrationOpen?: boolean }) {
  const client = useQueryClient()
  const identity = useQuery({ queryKey: studioIdentityKey, queryFn: getStudioIdentity, retry: false, refetchInterval: 30000 })
  const registration = useQuery({ queryKey: ['studio-registration'], queryFn: getStudioRegistration, enabled: !authPage && identity.error instanceof ApiError && identity.error.status === 401 })
  if (identity.isLoading) return <div className='flex h-svh items-center justify-center'><Skeleton className='h-72 w-96' /></div>
  if (!identity.isError && identity.data?.must_change_password) return <div className='flex h-svh items-center justify-center p-5'><StudioPasswordSettings required /></div>
  if (!identity.isError && identity.data) return children
  if (identity.error && (!(identity.error instanceof ApiError) || identity.error.status !== 401)) return <ErrorBanner message={identity.error.message} onRetry={() => void identity.refetch()} />
  if (!authPage && registration.error) return <ErrorBanner message={registration.error.message} onRetry={() => void registration.refetch()} />
  const onAuthenticated = (user: StudioUser) => {
    replaceStudioIdentity(client, user)
  }
  if (authPage === 'register') return <StudioRegisterPage onRegister={onAuthenticated} />
  if (authPage === 'login') return <StudioLoginPage onLogin={onAuthenticated} registrationOpen={registrationOpen} />
  return <StudioLoginPage onLogin={onAuthenticated} registrationOpen={registration.data?.enabled === true} />
}

export function StudioProfileFields({ value, onChange }: { value: StudioProfile; onChange: (profile: StudioProfile) => void }) {
  return <FieldGroup><Field><FieldLabel htmlFor='studio-profile-nickname'>昵称</FieldLabel><Input id='studio-profile-nickname' value={value.nickname} onChange={event => onChange({ ...value, nickname: event.target.value })} /></Field><Field><FieldLabel htmlFor='studio-profile-email'>邮箱</FieldLabel><Input id='studio-profile-email' type='email' value={value.email} onChange={event => onChange({ ...value, email: event.target.value })} /></Field><Field><FieldLabel htmlFor='studio-profile-avatar'>头像地址</FieldLabel><Input id='studio-profile-avatar' type='url' value={value.avatar_url} onChange={event => onChange({ ...value, avatar_url: event.target.value })} /></Field></FieldGroup>
}

export function StudioAccountSettings() {
  const identity = useQuery({ queryKey: studioIdentityKey, queryFn: getStudioIdentity })
  if (!identity.data) return null
  return <div className='flex min-h-0 flex-1 flex-col gap-5 overflow-auto p-5'><StudioProfileSettings key={identity.data.updated_at} user={identity.data} /><StudioPasswordSettings /></div>
}

function StudioProfileSettings({ user }: { user: StudioUser }) {
  const client = useQueryClient()
  const [profile, setProfile] = useState<StudioProfile>({ nickname: user.nickname, email: user.email, avatar_url: user.avatar_url })
  const save = useMutation({ mutationFn: () => updateStudioProfile(profile), onSuccess: result => { client.setQueryData(studioIdentityKey, result); toast.success('资料已保存') } })
  return <Card><CardHeader><CardTitle><h2>个人资料</h2></CardTitle></CardHeader><CardContent><form id='studio-profile' onSubmit={event => { event.preventDefault(); save.mutate() }}><StudioProfileFields value={profile} onChange={setProfile} />{save.error && <ErrorBanner message={save.error.message} />}</form></CardContent><CardFooter><Button size='sm' form='studio-profile' disabled={save.isPending}>保存资料</Button></CardFooter></Card>
}

function StudioPasswordSettings({ required = false }: { required?: boolean }) {
  const client = useQueryClient()
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const mismatch = confirmation !== '' && confirmation !== newPassword
  const save = useMutation({ mutationFn: () => changeStudioPassword(currentPassword, newPassword), onSuccess: result => { client.setQueryData(studioIdentityKey, result); setCurrentPassword(''); setNewPassword(''); setConfirmation(''); toast.success('密码已保存') } })
  const logout = useMutation({ mutationFn: logoutStudio, onSuccess: () => { client.removeQueries({ queryKey: ['studio'] }); window.location.assign('/studio') } })
  return (
    <Card className={required ? 'w-full max-w-sm' : undefined}>
      <CardHeader><CardTitle><h2>{required ? '修改初始密码' : '修改密码'}</h2></CardTitle></CardHeader>
      <CardContent>
        <form id='studio-change-password' onSubmit={event => { event.preventDefault(); if (!mismatch) save.mutate() }}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor='studio-current-password'>当前密码</FieldLabel>
              <SecretInput id='studio-current-password' required autoComplete='current-password' value={currentPassword} onChange={event => setCurrentPassword(event.target.value)} />
            </Field>
            <Field>
              <FieldLabel htmlFor='studio-new-password'>新密码</FieldLabel>
              <SecretInput id='studio-new-password' required minLength={8} autoComplete='new-password' value={newPassword} onChange={event => setNewPassword(event.target.value)} />
            </Field>
            <Field data-invalid={mismatch || undefined}>
              <FieldLabel htmlFor='studio-confirm-password'>确认密码</FieldLabel>
              <SecretInput id='studio-confirm-password' required aria-invalid={mismatch || undefined} autoComplete='new-password' value={confirmation} onChange={event => setConfirmation(event.target.value)} />
              {mismatch && <p role='alert' className='text-sm text-destructive'>两次密码不一致</p>}
            </Field>
            {save.error && <ErrorBanner message={save.error.message} />}
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter className='gap-2'>
        <Button size='sm' form='studio-change-password' disabled={save.isPending || !confirmation || mismatch}>保存密码</Button>
        {required && <Button size='sm' variant='outline' disabled={logout.isPending} onClick={() => logout.mutate()}>退出登录</Button>}
      </CardFooter>
    </Card>
  )
}
