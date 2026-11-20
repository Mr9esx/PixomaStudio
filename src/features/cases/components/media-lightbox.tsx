import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

type Props = { open: boolean; onOpenChange: (open: boolean) => void; src?: string; isVideo: boolean; alt?: string }

export function MediaLightbox({ open, onOpenChange, src, isVideo, alt = '媒体预览' }: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[min(92vw,1100px)] border-0 bg-foreground p-2 sm:max-w-[min(92vw,1100px)] [&_[data-slot='dialog-close']]:size-8 [&_[data-slot='dialog-close']]:text-background [&_[data-slot='dialog-close']]:opacity-80 [&_[data-slot='dialog-close']]:hover:bg-background/60 [&_[data-slot='dialog-close']]:hover:opacity-100">
        <DialogTitle className='sr-only'>{alt}</DialogTitle>
        <DialogDescription className='sr-only'>查看媒体内容</DialogDescription>
        {isVideo ? <video src={src} controls autoPlay className='max-h-[85vh] w-full object-contain' />
          : <img src={src} alt={alt} className='max-h-[85vh] w-full object-contain' />}
      </DialogContent>
    </Dialog>
  )
}
