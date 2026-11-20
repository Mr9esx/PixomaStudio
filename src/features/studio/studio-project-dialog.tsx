import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import {
  createStudioProject,
  updateStudioProject,
  type StudioProject,
  type StudioProjectType,
} from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'

export function StudioProjectDialog({
  open,
  project,
  onOpenChange,
  onSaved,
}: {
  open: boolean
  project?: StudioProject
  onOpenChange: (open: boolean) => void
  onSaved?: (project: StudioProject) => void
}) {
  const queryClient = useQueryClient()
  const [name, setName] = useState(project?.name ?? '')
  const [projectType, setProjectType] = useState<StudioProjectType>(project?.project_type ?? 'general')
  const save = useMutation({
    mutationFn: () =>
      project
        ? updateStudioProject(project.id, name.trim(), projectType)
        : createStudioProject(name.trim(), projectType),
    onSuccess: async (saved) => {
      await queryClient.invalidateQueries({ queryKey: ['studio', 'projects'] })
      onOpenChange(false)
      onSaved?.(saved)
    },
  })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          onSubmit={(event) => {
            event.preventDefault()
            if (name.trim()) save.mutate()
          }}
        >
          <DialogHeader>
            <DialogTitle>{project ? '编辑项目' : '新建项目'}</DialogTitle>
            <DialogDescription className='sr-only'>设置项目名称与类型</DialogDescription>
          </DialogHeader>
          <FieldGroup className='mt-5 gap-4'>
            <Field>
              <FieldLabel htmlFor='studio-project-name'>项目名称</FieldLabel>
              <Input
                id='studio-project-name'
                disabled={save.isPending}
                autoFocus
                maxLength={80}
                required
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel id='studio-project-type-label'>项目类型</FieldLabel>
              <ToggleGroup type='single' variant='outline' size='sm' disabled={save.isPending} aria-labelledby='studio-project-type-label' value={projectType} onValueChange={(value) => { if (value === 'general' || value === 'video') setProjectType(value) }}>
                <ToggleGroupItem value='general' className='flex-1'>通用</ToggleGroupItem>
                <ToggleGroupItem value='video' className='flex-1'>视频</ToggleGroupItem>
              </ToggleGroup>
            </Field>
          </FieldGroup>
          {save.isError ? (
            <p role='alert' className='mt-2 text-sm text-destructive'>
              {save.error.message}
            </p>
          ) : null}
          <DialogFooter className='mt-5'>
            <Button
              type='button'
              variant='outline'
              size='sm'
              onClick={() => onOpenChange(false)}
            >
              取消
            </Button>
            <Button type='submit' size='sm' disabled={!name.trim() || save.isPending}>
              {save.isPending ? '保存中…' : '保存项目'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
