import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { copyFileSync, existsSync, mkdirSync, openSync, closeSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { availableParallelism } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const cache = join(root, '.local-cache')
const destination = join(root, 'src-tauri/binaries')
const sources = [
  { name: 'freetype', version: '2.14.1', archive: 'freetype-2.14.1.tar.xz', sha256: '32427e8c471ac095853212a37aef816c60b42052d4d9e48230bab3bdf2936ccc', url: 'https://downloads.sourceforge.net/project/freetype/freetype2/2.14.1/freetype-2.14.1.tar.xz', options: ['FT_DISABLE_ZLIB', 'FT_DISABLE_BZIP2', 'FT_DISABLE_PNG', 'FT_DISABLE_HARFBUZZ', 'FT_DISABLE_BROTLI'].map(name => `-D${name}=ON`), license: 'docs/FTL.TXT' },
  { name: 'harfbuzz', version: '12.2.0', archive: 'harfbuzz-12.2.0-release.tar.xz', sha256: 'ecb603aa426a8b24665718667bda64a84c1504db7454ee4cadbd362eea64e545', url: 'https://github.com/harfbuzz/harfbuzz/releases/download/12.2.0/harfbuzz-12.2.0.tar.xz', options: ['-DHB_HAVE_FREETYPE=ON', ...['HB_HAVE_GLIB', 'HB_HAVE_GOBJECT', 'HB_HAVE_INTROSPECTION', 'HB_HAVE_ICU', 'HB_HAVE_CAIRO', 'HB_HAVE_GRAPHITE2', 'HB_HAVE_CORETEXT', 'HB_BUILD_UTILS', 'HB_BUILD_SUBSET', 'BUILD_TESTING'].map(name => `-D${name}=OFF`)], license: 'COPYING' },
]

function prefixFor(target) { return join(cache, `media-fonts-static-${target}`) }

export function prepareMediaLibraries(target) {
  if (process.platform !== 'darwin') return { env: process.env, fingerprint: null }
  const prefix = prefixFor(target)
  const fingerprint = JSON.stringify({ sources, target, deployment: '11.0', shared: false })
  const stamp = join(prefix, 'build.json')
  mkdirSync(prefix, { recursive: true })
  mkdirSync(join(destination, 'media-libs-licenses'), { recursive: true })
  const env = { ...process.env, MACOSX_DEPLOYMENT_TARGET: '11.0', PKG_CONFIG_PATH: '', PKG_CONFIG_LIBDIR: join(prefix, 'lib/pkgconfig') }
  if (!existsSync(stamp) || readFileSync(stamp, 'utf8') !== fingerprint) {
    const log = openSync(join(cache, 'media-fonts-build.log'), 'w')
    try {
      for (const item of sources) {
        const archive = join(cache, item.archive)
        if (!existsSync(archive)) {
          const pending = `${archive}.pending`
          execFileSync('curl', ['--fail', '--location', '--retry', '3', '--connect-timeout', '30', '--output', pending, item.url], { stdio: 'inherit' })
          if (createHash('sha256').update(readFileSync(pending)).digest('hex') !== item.sha256) throw new Error(`${item.name} 源码校验失败`)
          renameSync(pending, archive)
        }
        if (createHash('sha256').update(readFileSync(archive)).digest('hex') !== item.sha256) throw new Error(`${item.name} 源码校验失败`)
        const source = join(cache, `${item.name}-${item.version}`)
        if (!existsSync(source)) execFileSync('tar', ['-xf', archive, '-C', cache], { stdio: 'inherit' })
        const build = join(cache, `${item.name}-static-build-${target}`)
        const options = ['-S', source, '-B', build, '-DCMAKE_BUILD_TYPE=Release', `-DCMAKE_INSTALL_PREFIX=${prefix}`, `-DCMAKE_PREFIX_PATH=${prefix}`, '-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0', `-DCMAKE_OSX_ARCHITECTURES=${process.arch === 'arm64' ? 'arm64' : 'x86_64'}`, '-DBUILD_SHARED_LIBS=OFF', ...item.options]
        console.log(`编译 ${item.name} ${item.version}（macOS 11.0）`)
        execFileSync('cmake', options, { env, stdio: ['ignore', log, 'inherit'] })
        execFileSync('cmake', ['--build', build, '--parallel', String(Math.min(8, availableParallelism()))], { env, stdio: ['ignore', log, 'inherit'] })
        execFileSync('cmake', ['--install', build], { env, stdio: ['ignore', log, 'inherit'] })
      }
    } finally { closeSync(log) }
    writeFileSync(stamp, fingerprint)
  }
  const licenses = join(destination, 'media-libs-licenses')
  for (const item of sources) copyFileSync(join(cache, `${item.name}-${item.version}`, item.license), join(licenses, `${item.name}-LICENSE.txt`))
  writeFileSync(join(licenses, 'SOURCES.txt'), sources.map(item => `${item.name} ${item.version}\n${item.url}\nSHA-256: ${item.sha256}\n部署目标: macOS 11.0\n链接方式: 静态链接\n`).join('\n'))
  return { env, fingerprint }
}

function dependencies(binary) {
  return execFileSync('otool', ['-L', binary], { encoding: 'utf8' }).split('\n').slice(1).filter(line => line.startsWith('\t')).map(line => line.trim().split(' (compatibility version')[0])
}

export function verifyMediaBinaries(target) {
  if (process.platform !== 'darwin') return
  const binaries = [join(destination, `ffmpeg-${target}`), join(destination, `ffprobe-${target}`)]
  for (const binary of binaries) {
    const platform = execFileSync('vtool', ['-show-build', binary], { encoding: 'utf8' })
    if (!/\bminos\s+11\.0\b/.test(platform)) throw new Error(`媒体程序的最低系统版本无效：${binary}`)
    for (const dependency of dependencies(binary)) {
      if (!dependency.startsWith('/usr/lib/') && !dependency.startsWith('/System/Library/')) throw new Error(`媒体程序包含非系统动态依赖：${dependency}`)
    }
    execFileSync('codesign', ['--force', '--sign', '-', binary], { stdio: 'ignore' })
    execFileSync('codesign', ['--verify', '--strict', binary], { stdio: 'ignore' })
  }
  for (const binary of binaries) execFileSync(binary, ['-version'], { stdio: 'ignore' })
}
