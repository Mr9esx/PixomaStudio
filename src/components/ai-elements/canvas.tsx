import { Background } from '@xyflow/react'
import '@xyflow/react/dist/style.css'

export function CanvasBackground() {
  return (
    <Background
      bgColor='var(--card)'
      color='color-mix(in oklch, var(--muted-foreground) 55%, var(--card))'
    />
  )
}
