import * as fs from 'fs'
import { tmpdir } from 'os'
import * as path from 'path'
import { readFileSync } from 'fs'
import { Output } from './type'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'

export function isUrl(s: string): boolean {
  return ['https://', 'http://'].some((i) => s.startsWith(i))
}
export const ArchiveFmtList = [
  '.tar',
  '.tbz2',
  '.tar.bz2',
  '.tgz',
  '.tar.gz',
  '.txz',
  '.tar.xz',
  '.tzstd',
  '.tzst',
  '.tar.zst',
  '.zip',
]
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

export function isExeFile(s: string): boolean {
  if (s.endsWith('.exe')) {
    return true
  }

  const reLatest =
    /https:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/latest\/download\/([^\/]+)/
  const reTag =
    /https:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/download\/([^\/]+)\/([^\/]+)/

  for (
    const [re, n] of [
      [reLatest, 3],
      [reTag, 4],
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
    const name = url.split('/').at(-1)!
    outputPath = path.join(tmpdir(), name)
  }
  const response = await fetch(url, getFetchOption())
  const buf = await response.arrayBuffer()
  fs.writeFileSync(outputPath, Buffer.from(buf))
  return outputPath
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
      }
    }
    case 'linux': {
      switch (arch) {
        case 'arm64': {
          if (musl) {
            return ['aarch64-unknown-linux-musl', 'aarch64-unknown-linux-gnu']
          }
          return ['aarch64-unknown-linux-gnu', 'aarch64-unknown-linux-musl']
        }
        case 'x64': {
          if (musl) {
            return ['x86_64-unknown-linux-musl', 'x86_64-unknown-linux-gnu']
          }
          return ['x86_64-unknown-linux-gnu', 'x86_64-unknown-linux-musl']
        }
      }
    }

    case 'win32': {
      switch (arch) {
        case 'x64': {
          return ['x86_64-pc-windows-msvc', 'x86_64-pc-windows-gnu']
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
  return s.endsWith('.sha256')
}
export function isMsiFile(s: string): boolean {
  return s.endsWith('.msi')
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

export function atomiInstall(src: string, dst: string) {
  const dir = path.dirname(dst)
  if (fs.existsSync(dir) && fs.statSync(dir).isFile()) {
    console.log(`remove ${dir}`)
    fs.rmSync(dir)
  }
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true })
  }
  fs.copyFileSync(src, dst)
}

export function isDistManfiest(s: string) {
  return s.endsWith('.json')
}

export function cleanPath(path: string): string {
  const parts = path.replaceAll('\\', '/').split('/')
  const stack: string[] = []

  for (const part of parts.filter((p) => p !== '' && p !== '.')) {
    if (part === '..') {
      if (stack.length > 0) {
        stack.pop()
      }
    } else {
      stack.push(part)
    }
  }

  return (parts[0] === '' ? '/' : '') + stack.join('/')
}

function modeToString(mode: number, isDir: boolean): string {
  // if (mode < 0 || mode > 0o777) {
  //   throw new Error('Invalid mode: must be in range 0 to 0o777')
  // }

  const rwxMapping = [
    '---',
    '--x',
    '-w-',
    '-wx',
    'r--',
    'r-x',
    'rw-',
    'rwx',
  ]

  const owner = rwxMapping[(mode >> 6) & 0b111]
  const group = rwxMapping[(mode >> 3) & 0b111]
  const others = rwxMapping[mode & 0b111]
  const d = isDir ? 'd' : '-'
  return `${d}${owner}${group}${others}`
}

function humanSize(bytes: number): string {
  if (bytes < 0) {
    throw new Error('Size must be non-negative')
  }

  const units = ['', 'K', 'M', 'G', 'T', 'P', 'E', 'Z', 'Y']
  let index = 0
  let size = bytes

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024
    index++
  }

  return `${parseFloat(size.toPrecision(2))}${units[index]}`
}

export function displayOutput(output: Output) {
  const s: string[] = []
  for (const v of Object.values(output)) {
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
  return s.join('\n')
}

export function showSuccess() {
  console.log('Installation Successful')
}

export function addOutputToPath(output: Output) {
  for (const v of Object.values(output)) {
    for (const p of [v.binDir, v.installDir]) {
      if (p && !hasPath(p)) {
        const sh = addPath(p)
        if (sh) {
          console.log(`Successfully added ${p} to ${sh}'s $PATH`)
        } else {
          console.log(`You need to add ${p} to your $PATH`)
        }
        if (isGithub()) {
          addGithubPath(p)
        }
      }
    }

    if (v.files.length === 1 && v.files[0].installPath) {
      const first = v.files[0].installPath
      if (first) {
        addExecutePermission(first)
      }
    }
  }
}
