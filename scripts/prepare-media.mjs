import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, openSync, closeSync, readFileSync, writeFileSync, copyFileSync, renameSync } from 'node:fs'
import { availableParallelism } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { prepareMediaLibraries, verifyMediaBinaries } from './prepare-media-libs.mjs'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const cache = join(root, '.local-cache')
const destination = join(root, 'src-tauri/binaries')
const version = '9.0.2'
const checksum = '8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'
const archive = join(cache, `ffmpeg-${version}.tar.xz`)
const source = join(cache, `ffmpeg-${version}`)
const target = execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim()
const media = prepareMediaLibraries(target)
const binary = join(destination, `ffmpeg-${target}${process.platform === 'win32' ? '.exe' : ''}`)
const probe = join(destination, `ffprobe-${target}${process.platform === 'win32' ? '.exe' : ''}`)
const stamp = join(cache, `ffmpeg-${target}.json`)
const options = [
  '--disable-autodetect', '--disable-network', '--disable-doc', '--disable-debug',
  '--disable-shared', '--enable-static', '--disable-gpl', '--disable-nonfree',
  '--disable-avdevice', '--disable-ffplay',
  '--disable-encoders', '--enable-encoder=png,aac,pcm_s16le',
  '--disable-muxers', '--enable-muxer=image2,mov,mp4,ipod,wav',
  '--disable-protocols', '--enable-protocol=file,pipe',
  '--disable-filters', '--enable-filter=scale,format,null,transpose,hflip,vflip,rotate,pad,fps,setsar,drawtext,apad,volume,atrim,asetpts,adelay,amix,alimiter,anullsrc,aresample,aformat,anull',
  '--enable-zlib', '--enable-libfreetype', '--enable-libharfbuzz',
  '--pkg-config-flags=--static',
]
if (process.platform === 'darwin') {
  options.push('--enable-videotoolbox', '--enable-encoder=h264_videotoolbox', '--extra-cflags=-mmacosx-version-min=11.0', '--extra-ldflags=-mmacosx-version-min=11.0')
} else {
  options.push('--enable-libopenh264', '--enable-encoder=libopenh264')
}
const configuration = JSON.stringify({ version, checksum, target, options, media: media.fingerprint })
mkdirSync(cache, { recursive: true })
mkdirSync(destination, { recursive: true })
if (!existsSync(archive) && existsSync(join(destination, 'FFmpeg-source.tar.xz'))) {
  copyFileSync(join(destination, 'FFmpeg-source.tar.xz'), archive)
}
if (!existsSync(archive)) {
  const pending = `${archive}.pending`
  execFileSync('curl', ['--fail', '--location', '--retry', '3', '--retry-all-errors', '--output', pending, `https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz`], { stdio: 'inherit' })
  renameSync(pending, archive)
}
if (createHash('sha256').update(readFileSync(archive)).digest('hex') !== checksum) {
  throw new Error('FFmpeg 源码校验失败')
}
if (!existsSync(source)) {
  execFileSync('tar', ['-xf', archive, '-C', cache], { stdio: 'inherit' })
}
if (!existsSync(binary) || !existsSync(probe) || !existsSync(stamp) || readFileSync(stamp, 'utf8') !== configuration) {
  const build = join(cache, `ffmpeg-build-${target}`)
  mkdirSync(build, { recursive: true })
  const log = openSync(join(cache, 'ffmpeg-build.log'), 'w')
  try {
    if (existsSync(join(build, 'Makefile'))) {
      execFileSync('make', ['clean'], { cwd: build, stdio: ['ignore', log, 'inherit'] })
    }
    console.log(`配置 FFmpeg ${version}`)
    execFileSync(join(source, 'configure'), options, { cwd: build, env: media.env, stdio: ['ignore', log, 'inherit'] })
    console.log(`编译 FFmpeg ${version}`)
    execFileSync('make', [`-j${Math.min(8, availableParallelism())}`, 'ffmpeg', 'ffprobe'], { cwd: build, stdio: ['ignore', log, 'inherit'] })
    copyFileSync(join(build, 'ffmpeg'), binary)
    copyFileSync(join(build, 'ffprobe'), probe)
  } finally {
    closeSync(log)
  }
  writeFileSync(stamp, configuration)
}
verifyMediaBinaries(target)
const license = execFileSync(binary, ['-L'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })
if (!license.includes('GNU Lesser General Public') || !license.includes('version 2.1 of the License')) {
  throw new Error('FFmpeg 许可证检查失败')
}
copyFileSync(join(source, 'COPYING.LGPLv2.1'), join(destination, 'FFmpeg-LICENSE.txt'))
copyFileSync(archive, join(destination, 'FFmpeg-source.tar.xz'))
writeFileSync(join(destination, 'FFmpeg-README.txt'), `FFmpeg ${version}\n源码 SHA-256：${checksum}\n构建目标：${target}\n构建参数：\n${options.join('\n')}\n\n构建使用官方发布源码。许可证与第三方说明：\n\n${readFileSync(join(source, 'LICENSE.md'), 'utf8')}`)
console.log(`FFmpeg ${version} 已准备完成`)
