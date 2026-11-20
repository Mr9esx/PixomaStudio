import { ArrowDownIcon, ArrowUpIcon, CaretSortIcon, EyeNoneIcon } from '@radix-ui/react-icons'
import type { Column } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'

export function DataTableColumnHeader<TData, TValue>({ column, title, className }: React.HTMLAttributes<HTMLDivElement> & { column: Column<TData, TValue>; title: string }) {
  const { t } = useTranslation()
  if (!column.getCanSort()) return <div className={cn('px-2', className)}>{title}</div>
  return <div className={cn('flex items-center gap-2', className)}><DropdownMenu>
    <DropdownMenuTrigger asChild><Button variant='ghost' size='sm' className='h-8 data-[state=open]:bg-accent'>
      <span>{title}</span>{column.getIsSorted() === 'desc' ? <ArrowDownIcon /> : column.getIsSorted() === 'asc' ? <ArrowUpIcon /> : <CaretSortIcon />}
    </Button></DropdownMenuTrigger>
    <DropdownMenuContent align='start'><DropdownMenuGroup>
      <DropdownMenuItem onClick={() => column.toggleSorting(false)}><ArrowUpIcon />{t('dataTable.asc')}</DropdownMenuItem>
      <DropdownMenuItem onClick={() => column.toggleSorting(true)}><ArrowDownIcon />{t('dataTable.desc')}</DropdownMenuItem>
      {column.getCanHide() ? <><DropdownMenuSeparator /><DropdownMenuItem onClick={() => column.toggleVisibility(false)}><EyeNoneIcon />{t('dataTable.hide')}</DropdownMenuItem></> : null}
    </DropdownMenuGroup></DropdownMenuContent>
  </DropdownMenu></div>
}
