import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { clearStudioSessions, listStudioSessions } from '@/lib/api/studio'
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { LoadingSkeleton } from '@/components/feedback/loading-skeleton'

export function StudioSessionManagement({
  onSessionsCleared,
}: {
  onSessionsCleared?: () => void
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [confirmation, setConfirmation] = useState('')
  const sessions = useQuery({
    queryKey: ['studio', 'sessions', 'management'],
    queryFn: () => listStudioSessions({ limit: 1 }),
  })
  const clearSessions = useMutation({
    mutationFn: clearStudioSessions,
    onSuccess: async () => {
      await queryClient.cancelQueries({ queryKey: ['studio', 'sessions'] })
      await queryClient.cancelQueries({ queryKey: ['studio', 'session'] })
      queryClient.removeQueries({ queryKey: ['studio', 'session'] })
      queryClient.removeQueries({
        queryKey: ['studio', 'sessions'],
        type: 'inactive',
      })
      queryClient.setQueryData(['studio', 'sessions', 'management'], [])
      window.localStorage.removeItem('studio.viewedRunIds')
      onSessionsCleared?.()
      await queryClient.invalidateQueries({ queryKey: ['studio', 'sessions'] })
      await queryClient.invalidateQueries({ queryKey: ['studio', 'library'] })
      setOpen(false)
      setConfirmation('')
      toast.success(t('studioOperations.cleared'))
    },
  })

  return (
    <AlertDialog
      open={open}
      onOpenChange={(value) => {
        if (clearSessions.isPending) return
        setOpen(value)
        if (!value) {
          setConfirmation('')
          clearSessions.reset()
        }
      }}
    >
      <section className='relative flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pt-3 pb-5'>
        <div className='mx-auto flex w-full max-w-4xl flex-col gap-3'>
          <h2 className='text-lg font-semibold'>通用</h2>
          <div className='flex flex-wrap items-center justify-between gap-3'>
            <h3 className='text-sm font-medium'>对话数据</h3>
            <AlertDialogTrigger asChild>
              <Button
                size='sm'
                variant='destructive'
                disabled={
                  !sessions.data?.length ||
                  clearSessions.isPending ||
                  sessions.isError
                }
              >
                {t('studioOperations.clear')}
              </Button>
            </AlertDialogTrigger>
          </div>
          {sessions.isPending ? (
            <LoadingSkeleton rows={1} />
          ) : sessions.isError ? (
            <ErrorBanner
              message={sessions.error.message}
              onRetry={() => void sessions.refetch()}
            />
          ) : null}
        </div>
      </section>
      <AlertDialogContent className='bg-popover'>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('studioOperations.clear')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('studioOperations.clearDescription')}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor='clear-studio-sessions-confirmation'>
              {t('studioOperations.confirmLabel')}
            </FieldLabel>
            <Input
              id='clear-studio-sessions-confirmation'
              autoComplete='off'
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
              disabled={clearSessions.isPending}
            />
          </Field>
        </FieldGroup>
        {clearSessions.isError ? (
          <p role='alert' className='text-sm text-destructive'>
            {clearSessions.error.message}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button
              size='sm'
              variant='outline'
              disabled={clearSessions.isPending}
            >
              {t('common.cancel')}
            </Button>
          </AlertDialogCancel>
          <Button
            size='sm'
            variant='destructive'
            disabled={confirmation !== '确认清空' || clearSessions.isPending}
            onClick={() => clearSessions.mutate()}
          >
            {t(
              clearSessions.isPending
                ? 'studioOperations.clearing'
                : 'studioOperations.clear'
            )}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
