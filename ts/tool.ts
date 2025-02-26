import * as fs from 'fs'
import { tmpdir } from 'os'
import * as path from 'path'
import { chmodSync, existsSync, mkdirSync, readFileSync } from 'fs'
import { Output, OutputFile } from './type'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'
import { humanSize, modeToString } from '@easy-install/easy-archive'
import { spawnSync } from 'child_process'
import { randomId } from './download'
import { dirname } from 'path'
import { getLocalTarget, guessTarget, targetToString } from 'guess-target'
import {
  ALL_FILE_EXTS,
  ArchiveFmtList,
  INSTALLER_EXTS,
  MAX_FILE_COUNT,
  SKIP_FMT_LIST,
  TEXT_FILE_EXTS,
  WINDOWS_EXE_EXTS,
} from './const'

export function isUrl(s: string): boolean {
  return ['https://', 'http://'].some((i) => s.startsWith(i))
}

export function isArchiveFile(s: string): boolean {
  return ArchiveFmtList.some((i) => s.toLowerCase().endsWith(i.toLowerCase()))
}

export function isSkip(s: string) {
  return [...SKIP_FMT_LIST, ...TEXT_FILE_EXTS, ...INSTALLER_EXTS].some((i) =>
    s.toLowerCase().endsWith(i.toLowerCase())
  )
}

export function endsWithExe(s: string): boolean {
  return WINDOWS_EXE_EXTS.some((i) => s.endsWith(i))
}

export function isExeUrl(s: string): boolean {
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

export function isDistManfiest(s: string) {
  return s.endsWith('.json')
}

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
  for (const i of ALL_FILE_EXTS) {
    if (s.toLowerCase().endsWith(i.toLowerCase())) {
      return s.slice(0, s.length - i.length)
    }
  }
  return s.split('.')[0]
}

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
