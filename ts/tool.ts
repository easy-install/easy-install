import * as fs from 'fs'
import { execSync } from 'child_process'
import { tmpdir } from 'os'
import * as path from 'path'
import { readFileSync } from 'fs'
import { decode, Files, Fmt, guess, File } from '@easy-install/easy-archive'

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

export function toMsysPath(s: string): string {
  s = s.replaceAll('\\', '/')
  s = s.replace(/^([A-Za-z]):\//, (_, drive) => `/${drive.toLowerCase()}/`)
  return s
}

export function randomId() {
  return Math.random().toString(36).slice(2)
}

export function createFiles(dir: string): Files {
  const files = Files.new()
  async function dfs(currentPath: string) {
    const entries = fs.readdirSync(currentPath);
    for (const entry of entries) {
      const fullPath = path.join(currentPath, entry);
      const stat = fs.statSync(fullPath);
      if (stat.isDirectory()) {
        // ignore empty dir
        dfs(fullPath);
      } else if (stat.isFile()) {
        const relativePath = path.relative(dir, fullPath).replaceAll('\\', '/');
        const buffer = fs.readFileSync(fullPath);
        const file = File.new(relativePath, buffer, stat.mode)
        files.insert(relativePath, file)
      }
    }
  }
  dfs(dir);
  return files;
}

export function extractToByShell(compressedFilePath: string, outputDir?: string): { outputDir: string; files?: Files } {
  const tmpDir = path.join(tmpdir(), randomId())
  let oriDir = outputDir ?? tmpDir
  const needCopy = !!outputDir

  outputDir = tmpDir
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true })
  }

  if (isMsys() && !compressedFilePath.endsWith('.zip')) {
    compressedFilePath = toMsysPath(compressedFilePath)
    outputDir = toMsysPath(outputDir)
  }
  if (!fs.existsSync(oriDir)) {
    fs.mkdirSync(oriDir, { recursive: true })
  }
  const rules = [
    {
      ext: ['.zip'],
      cmd: process.platform !== 'win32'
        ? `unzip -o "${compressedFilePath}" -d "${outputDir}"`
        : `powershell -c "Expand-Archive -Path ${compressedFilePath} -DestinationPath  ${outputDir} -Force"`,
    },
    {
      ext: ['.tar', '.tar.xz'],
      cmd: `tar -xf "${compressedFilePath}" -C "${outputDir}"`,
    },
    {
      ext: ['.tar.gz', '.tgz'],
      cmd: `tar -xzvf "${compressedFilePath}" -C "${outputDir}"`,
    },
    {
      ext: ['.tar.bz2'],
      cmd: `tar -xjf "${compressedFilePath}" -C "${outputDir}"`,
    },
    { ext: ['.7z'], cmd: `7z x "${compressedFilePath}" -o"${outputDir}"` },
    { ext: ['.rar'], cmd: `unrar x "${compressedFilePath}" "${outputDir}"` },
    { ext: ['.rar'], cmd: `unrar x "${compressedFilePath}" "${outputDir}"` },
  ] as const

  for (const { ext, cmd } of rules) {
    for (const e of ext) {
      if (compressedFilePath.endsWith(e)) {
        execSync(cmd)
      }
    }
  }
  const files = createFiles(outputDir)
  if (needCopy && tmpDir !== oriDir) {
    fs.cpSync(tmpDir, oriDir, { recursive: true })
  }
  return { outputDir: oriDir, files }
}

export function extractToByWasm(
  compressedFilePath: string,
  outputDir?: string,
): { outputDir: string; files?: Files } {
  const fmt = guess(compressedFilePath)
  if (!outputDir) {
    outputDir = path.join(tmpdir(), randomId())
    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true })
    }
  }
  if (!fmt) {
    console.log('extractTo not support this file type')
    return { outputDir }
  }
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true })
  }
  const buf = new Uint8Array(readFileSync(compressedFilePath))
  const files = decode(fmt, buf)
  if (!files) {
    console.log('failed to decode')
    return { outputDir }
  }
  for (const i of files.keys()) {
    const file = files.get(i)
    if (!file) {
      continue
    }
    const filePath = file.get_path()
    const mode = file.get_mode()
    const buffer = file.get_buffer()

    if (filePath.endsWith('/') || !buffer.length) {
      continue
    }

    const outputPath = path.join(outputDir, filePath)
    const dir = path.dirname(outputPath)
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true })
    }
    fs.writeFileSync(outputPath, buffer)

    if (mode && process.platform !== 'win32') {
      fs.chmodSync(outputPath, mode)
    }
  }
  return { outputDir, files }
}

export function extractTo(compressedFilePath: string, outputDir?: string) {
  try {
    return extractToByWasm(compressedFilePath, outputDir)
  } catch {
    return extractToByShell(compressedFilePath, outputDir)
  }
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
    fs.chmodSync(filePath, 0o755)
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
