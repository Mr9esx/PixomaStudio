import { ChevronLeftIcon, ChevronRightIcon, DoubleArrowLeftIcon, DoubleArrowRightIcon } from '@radix-ui/react-icons'
import type { Table } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { cn, getPageNumbers } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

export function DataTablePagination<TData>({ table, className }: { table: Table<TData>; className?: string }) {
  const { t } = useTranslation()
  const currentPage = table.getState().pagination.pageIndex + 1
  const totalPages = Math.max(1, table.getPageCount())
  const pageLabel = t('dataTable.pageOf', { page: currentPage, count: totalPages })
  return <div className={cn('flex items-center justify-between @max-2xl/content:flex-col-reverse @max-2xl/content:gap-4', className)}>
    <div className='flex w-full items-center justify-between'>
      <div className='flex w-25 items-center justify-center text-sm font-medium @2xl/content:hidden'>{pageLabel}</div>
      <div className='flex items-center gap-2 @max-2xl/content:flex-row-reverse'>
        <Select value={`${table.getState().pagination.pageSize}`} onValueChange={(value) => table.setPageSize(Number(value))}>
          <SelectTrigger className='h-8 w-17.5' aria-label={t('dataTable.rowsPerPage')}><SelectValue /></SelectTrigger>
          <SelectContent side='top'><SelectGroup>{[10, 20, 30, 40, 50].map((size) => <SelectItem key={size} value={`${size}`}>{size}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
        <p className='hidden text-sm font-medium sm:block'>{t('dataTable.rowsPerPage')}</p>
      </div>
    </div>
    <div className='flex items-center gap-6 lg:gap-8'>
      <div className='flex w-25 items-center justify-center text-sm font-medium @max-3xl/content:hidden'>{pageLabel}</div>
      <div className='flex items-center gap-2'>
        <Button size='icon-sm' variant='outline' className='@max-md/content:hidden' aria-label={t('dataTable.goToFirstPage')} onClick={() => table.setPageIndex(0)} disabled={!table.getCanPreviousPage()}><DoubleArrowLeftIcon /></Button>
        <Button size='icon-sm' variant='outline' aria-label={t('dataTable.goToPreviousPage')} onClick={() => table.previousPage()} disabled={!table.getCanPreviousPage()}><ChevronLeftIcon /></Button>
        {getPageNumbers(currentPage, totalPages).map((number, index) => <div key={`${number}-${index}`} className='flex items-center'>
          {number === '...' ? <span className='px-1 text-sm text-muted-foreground'>…</span> : <Button size='sm' variant={currentPage === number ? 'default' : 'outline'}
            className='min-w-8 px-2 tabular-nums' aria-label={t('dataTable.goToPage', { page: number })} aria-current={currentPage === number ? 'page' : undefined}
            onClick={() => table.setPageIndex(Number(number) - 1)}>{number}</Button>}
        </div>)}
        <Button size='icon-sm' variant='outline' aria-label={t('dataTable.goToNextPage')} onClick={() => table.nextPage()} disabled={!table.getCanNextPage()}><ChevronRightIcon /></Button>
        <Button size='icon-sm' variant='outline' className='@max-md/content:hidden' aria-label={t('dataTable.goToLastPage')} onClick={() => table.setPageIndex(totalPages - 1)} disabled={!table.getCanNextPage()}><DoubleArrowRightIcon /></Button>
      </div>
    </div>
  </div>
}
