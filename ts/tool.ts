import * as fs from 'fs'
import { tmpdir } from 'os'
import * as path from 'path'
import { chmodSync, existsSync, mkdirSync, readFileSync } from 'fs'
import { Output, OutputFile } from './type'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'
import { humanSize, modeToString } from '@easy-install/easy-archive'
import { spawnSync } from 'child_process'
import { randomId } from './download'
import { extensions, Fmt } from '@easy-install/easy-archive'
import { dirname } from 'path'
import { getLocalTarget, guessTarget, targetToString } from 'guess-target'

export function isUrl(s: string): boolean {
  return ['https://', 'http://'].some((i) => s.startsWith(i))
}

const ArchiveFmtList = [
  Fmt.Tar,
  Fmt.TarBz,
  Fmt.TarGz,
  Fmt.TarXz,
  Fmt.TarZstd,
  Fmt.Zip,
].map((i) => extensions(i)).flat()

export function isArchiveFile(s: string): boolean {
  for (
    const i of ArchiveFmtList
  ) {
    if (s.endsWith(i)) {
      return true
    }
  }
  return false
}

export function endsWithExe(s: string): boolean {
  return WINDOWS_EXE_EXTS.some((i) => s.endsWith(i))
}

export function isExeFile(s: string): boolean {
  if (endsWithExe(s)) {
    return true
  }

  // https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64
  const reLatest =
    /https?:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/latest\/download\/([^\/]+)$/
  const reTag =
    /https?:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/download\/([^\/]+)\/([^\/]+)$/
  const reTag2 =
    /https?:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/download\/([^\/]+)\/([^\/]+)\/([^\/]+)$/

  for (
    const [re, n] of [
      [reTag2, 5],
      [reTag, 4],
      [reLatest, 3],
    ] as const
  ) {
    const match = re.exec(s)
    const name = match?.[n]
    if (name && !isArchiveFile(name) && !name.includes('.')) {
      return true
    }
  }
  return false
}

export function getFetchOption() {
  const headers: HeadersInit = {
    'User-Agent': 'GitHub Actions',
    Connection: 'close',
  }
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `token ${process.env.GITHUB_TOKEN}`
  }
  return {
    headers,
  }
}

export async function download(url: string, outputPath?: string) {
  if (!outputPath) {
    const name = url.split('/').at(-1) || randomId()
    outputPath = path.join(tmpdir(), name)
  }
  const response = await fetch(url, getFetchOption())
  const buf = await response.arrayBuffer()
  fs.writeFileSync(outputPath, Buffer.from(buf))
  return outputPath.replaceAll('\\', '/')
}

export function detectTargets(
  platform = process.platform,
  arch = process.arch,
  musl = isMusl(),
): string[] {
  switch (platform) {
    case 'darwin': {
      switch (arch) {
        case 'arm64': {
          return ['aarch64-apple-darwin']
        }
        case 'x64': {
          return ['x86_64-apple-darwin']
        }
        default: {
          return []
        }
      }
    }
    case 'linux': {
      switch (arch) {
        case 'ia32': {
          if (musl) {
            return ['i686-unknown-linux-musl']
          }
          return ['i686-unknown-linux-gnu']
        }
        case 'arm': {
          if (musl) {
            return ['arm-unknown-linux-musleabihf']
          }
          return ['arm-unknown-linux-gnu']
        }
        case 'arm64': {
          if (musl) {
            return ['aarch64-unknown-linux-musl' // 'aarch64-unknown-linux-gnu'
            ]
          }
          return ['aarch64-unknown-linux-gnu' // 'aarch64-unknown-linux-musl'
          ]
        }
        case 'x64': {
          if (musl) {
            return ['x86_64-unknown-linux-musl' // 'x86_64-unknown-linux-gnu'
            ]
          }
          return ['x86_64-unknown-linux-gnu' //  'x86_64-unknown-linux-musl'
          ]
        }
        default: {
          return []
        }
      }
    }
    case 'win32': {
      switch (arch) {
        case 'x64': {
          return ['x86_64-pc-windows-msvc', 'x86_64-pc-windows-gnu']
        }
        case 'arm64': {
          return [
            'aarch64-pc-windows-msvc',
          ]
        }
        case 'ia32': {
          return [
            'i686-pc-windows-msvc',
          ]
        }
        default: {
          return []
        }
      }
    }
    case 'freebsd': {
      switch (arch) {
        case 'x64': {
          return [
            'x86_64-unknown-freebsd',
          ]
        }
      }
    }
  }
  return []
}

export function getAssetNames(
  name: string,
  platform = process.platform,
  arch = process.arch,
  musl = isMusl(),
): string[] {
  return detectTargets(platform, arch, musl).map((i) => `${name}-${i}`)
}

export function getBinName(bin: string) {
  return process.platform === 'win32' && !bin.endsWith('.exe') &&
      !bin.includes('.')
    ? `${bin}.exe`
    : bin
}

export function parseDownloadUrl(url: string) {
  const regex =
    /https:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\/download\/([^/]+)\/(.+)/
  const match = url.match(regex)

  if (match) {
    const [, owner, repo, tag, name] = match
    return {
      owner,
      repo,
      tag,
      name,
    }
  }
}

export function isMusl() {
  let musl = false
  if (process.platform === 'linux') {
    musl = isMuslFromFilesystem()
    if (musl === null) {
      musl = isMuslFromReport()
    }
    if (musl === null) {
      musl = isMuslFromChildProcess()
    }
  }
  return musl
}

const isFileMusl = (f: string) =>
  f.includes('libc.musl-') || f.includes('ld-musl-')

const isMuslFromFilesystem = () => {
  try {
    return readFileSync('/usr/bin/ldd', 'utf-8').includes('musl')
  } catch {
    return false
  }
}

const isMuslFromReport = () => {
  const report: any = typeof process.report.getReport === 'function'
    ? process.report.getReport()
    : null
  if (!report) {
    return false
  }
  if (report.header?.glibcVersionRuntime) {
    return false
  }
  if (Array.isArray(report.sharedObjects)) {
    if (report.sharedObjects.some(isFileMusl)) {
      return true
    }
  }
  return false
}

const isMuslFromChildProcess = () => {
  try {
    return require('child_process')
      .execSync('ldd --version', { encoding: 'utf8' })
      .includes('musl')
  } catch (e) {
    // If we reach this case, we don't know if the system is musl or not, so is better to just fallback to false
    return false
  }
}

export function removePostfix(s: string): string {
  for (const i of ArchiveFmtList) {
    if (i && s.endsWith(i)) {
      return s.slice(0, s.length - i.length)
    }
  }
  return s
}

export function matchTargets(targets: string[], platformTargets: string[]) {
  for (const i of platformTargets) {
    if (targets.includes(i)) {
      return i
    }
  }
  return undefined
}

export function replaceFilename(baseUrl: string, name: string): string {
  const i = baseUrl.lastIndexOf('/')
  return i !== -1 ? baseUrl.slice(0, i + 1) + name : name
}

export function isHashFile(s: string): boolean {
  return ['.sha256sum', '.sha256'].some((i) =>
    s.toLowerCase().endsWith(i.toLowerCase())
  )
}
export function isMsiFile(s: string): boolean {
  return ['.msi', '.app', '.msix', 'appimage'].some((i) =>
    s.toLowerCase().endsWith(i.toLowerCase())
  )
}
export function isMsys() {
  return !!process.env['MSYSTEM']
}

export function addExecutePermission(filePath: string) {
  try {
    const meta = fs.statSync(filePath)
    if (meta.isDirectory()) {
      return
    }
    fs.chmodSync(filePath, meta.mode | 0o111)
  } catch (error) {
    console.error('Failed to add execute permission', error)
  }
}

// export function atomiInstall(src: string, dst: string) {
//   const dir = path.dirname(dst)
//   if (fs.existsSync(dir) && fs.statSync(dir).isFile()) {
//     console.log(`remove ${dir}`)
//     fs.rmSync(dir)
//   }
//   if (!fs.existsSync(dir)) {
//     fs.mkdirSync(dir, { recursive: true })
//   }
//   fs.copyFileSync(src, dst)
// }

export function isDistManfiest(s: string) {
  return s.endsWith('.json')
}

const MAX_FILE_COUNT = 16
export function displayOutput(output: Output) {
  const s: string[] = []
  for (const v of Object.values(output)) {
    if (v.files.length > MAX_FILE_COUNT) {
      const sumSize = v.files.reduce((pre, cur) => pre + cur.size, 0)
      s.push([
        humanSize(sumSize),
        `(total ${v.files.length})`,
        v.installDir,
      ].join(' '))
    } else {
      const maxSizeLen = v.files.reduce(
        (pre, cur) => Math.max(pre, humanSize(cur.size).length),
        0,
      )
      for (const i of v.files) {
        s.push([
          modeToString(i.mode, i.isDir),
          humanSize(i.size).padStart(maxSizeLen, ' '),
          [i.originPath, i.installPath].join(' -> '),
        ].join(' '))
      }
    }
  }
  return s.join('\n')
}

export function showSuccess() {
  console.log('Installation Successful')
}

function addToPath(p: string) {
  if (p && !hasPath(p)) {
    const sh = addPath(p)
    if (sh) {
      console.log(`Successfully added ${p} to ${sh}'s $PATH`)
    } else {
      console.log(`You need to add ${p} to your $PATH`)
    }
    if (isGithub()) {
      addGithubPath(p)
      console.log(`Successfully added ${p} to github's $PATH`)
    }
  }
}

function which(s: string): string | undefined {
  const [name, args] = process.platform === 'win32'
    ? [
      'powershell',
      ['-c', `(get-command ${s}).Source`],
    ]
    : ['which', [s]]

  const ret = spawnSync(name, args)
  if (!!ret.status || !ret.stdout) {
    return
  }
  return ret.stdout.toString().trim().replaceAll('\\', '/')
}
const EXEC_MASK = 0o111

function executabe(name: string, mode: number): boolean {
  return name.endsWith('.exe') ||
    (!name.includes('.') && (mode & EXEC_MASK) !== 0)
}

export function getFilename(url: string): string {
  const i = url.lastIndexOf('/')
  return i === -1 ? url : url.slice(i + 1)
}
function check(file: OutputFile): boolean {
  const fp = file.installPath.toLowerCase()
  const name = getFilename(fp)!
  if (
    !executabe(name, file.mode)
  ) {
    return false
  }

  const whichPath = which(name)?.toLowerCase()
  return !!whichPath?.length && fp !== whichPath
}

export function addOutputToPath(output: Output) {
  for (const { files } of Object.values(output)) {
    for (const f of files) {
      const deep = f.originPath.split('/').length
      if (deep <= 3 && check(f)) {
        console.log(`Warning: file exists at ${f.installPath}`)
      }
    }
  }
  for (const { installDir, files } of Object.values(output)) {
    addToPath(installDir)

    for (const f of files) {
      const deep = f.originPath.split('/').length
      // FIXME: ignore node_modules
      if (
        deep <= 3 && (WINDOWS_EXE_EXTS.some((i) => f.originPath.endsWith(i)) ||
          (f.mode && (f.mode & 0o111) !== 0))
      ) {
        const dir = dirname(f.installPath)
        addToPath(dir)
      }
    }

    if (files.length === 1 && files[0].installPath) {
      const first = files[0].installPath
      if (first) {
        addExecutePermission(first)
      }
    }
  }
}

export function getCommonPrefixLen(list: readonly string[]): number {
  if (list.length === 0) {
    return 0
  }
  if (list.length === 1) {
    const i = list[0].lastIndexOf('/')
    return i === -1 ? 0 : i + 1
  }
  const parts = list.map((i) => i.split('/'))
  const n = parts.reduce((pre, cur) => Math.max(pre, cur.length), 0)
  let p = 0
  while (p < n) {
    const head = parts.map((k) => k[p])
    if (head.some((i) => i !== head[0])) {
      break
    }
    p++
  }
  const s = parts[0].slice(0, p).join('/').length
  return s === 0 ? 0 : s + 1
}

export function installOutputFiles(files: OutputFile[]) {
  for (const { installPath, buffer, mode } of files) {
    const dstDir = dirname(installPath)
    if (!existsSync(dstDir)) {
      mkdirSync(dstDir, { recursive: true })
    }
    fs.writeFileSync(installPath, buffer)
    if (mode) {
      chmodSync(installPath, mode)
    }
  }
}

export function nameNoExt(s: string) {
  return s.split('.')[0]
}

export const WINDOWS_EXE_EXTS = ['.exe', '.ps1', '.bat']
export const INSTALLER_EXTS = ['.msi', '.is_msi_file']

export function guessName(
  name: string,
): { name: string; target: string } | undefined {
  const filename = getFilename(name)
  const s = nameNoExt(filename)
  const guess = guessTarget(s)
  const localTarget = getLocalTarget().map(targetToString)
  const ret = guess.find((i) => localTarget.includes(targetToString(i.target)))
  if (!ret) return
  return { name: ret.name, target: targetToString(ret.target) }
}
