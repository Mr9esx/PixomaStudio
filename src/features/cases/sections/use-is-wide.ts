import { useEffect, useState } from 'react'

export function useIsWide(): boolean {
  const [wide, setWide] = useState(() =>
    typeof window === 'undefined' ? false : window.matchMedia('(min-width: 920px)').matches
  )
  useEffect(() => {
    const m = window.matchMedia('(min-width: 920px)')
    const onChange = () => setWide(m.matches)
    onChange()
    m.addEventListener('change', onChange)
    return () => m.removeEventListener('change', onChange)
  }, [])
  return wide
}
