type ExitGuard = () => Promise<boolean>

let guard: ExitGuard | undefined
let pending: Promise<boolean> | undefined

export function registerDesktopExitGuard(next: ExitGuard) {
  guard = next
  return () => { if (guard === next) guard = undefined }
}

export function prepareDesktopExit(): Promise<boolean> {
  pending ??= Promise.resolve().then(async () => {
    while (true) {
      const current = guard
      if (current && !await current()) return false
      if (current === guard) return true
    }
  }).finally(() => { pending = undefined })
  return pending
}
