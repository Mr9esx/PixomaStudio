import { expect, it } from 'vitest'
import { prepareDesktopExit, registerDesktopExitGuard } from './desktop-exit'

it('退出等待保存结果，并合并同时到达的退出请求', async () => {
  const saving = Promise.withResolvers<boolean>()
  let attempts = 0
  const unregister = registerDesktopExitGuard(async () => { attempts++; return saving.promise })
  try {
    const first = prepareDesktopExit()
    const second = prepareDesktopExit()
    await Promise.resolve()
    expect(attempts).toBe(1)
    saving.resolve(false)
    expect(await first).toBe(false)
    expect(await second).toBe(false)
    expect(attempts).toBe(1)
  } finally { unregister() }
  expect(await prepareDesktopExit()).toBe(true)
})

it('保存期间切换编辑器后继续检查当前编辑器，旧清理函数保留新保护', async () => {
  const saving = Promise.withResolvers<boolean>()
  const previous = registerDesktopExitGuard(async () => saving.promise)
  const exiting = prepareDesktopExit()
  await Promise.resolve()
  const current = registerDesktopExitGuard(async () => false)
  previous()
  saving.resolve(true)
  try { expect(await exiting).toBe(false) } finally { current() }
  expect(await prepareDesktopExit()).toBe(true)
})

it('检查失败保留失败结果，后续退出可以重新检查', async () => {
  const unregister = registerDesktopExitGuard(async () => { throw new Error('保存失败') })
  try { await expect(prepareDesktopExit()).rejects.toThrow('保存失败') } finally { unregister() }
  expect(await prepareDesktopExit()).toBe(true)
})
