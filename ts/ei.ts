import { detectTargets } from 'detect-targets'
import { dirname, join } from 'path'
import { existsSync, mkdirSync, writeFileSync } from 'fs'
import { execFileSync } from 'child_process'
import { decode, guess } from '@easy-install/easy-archive'
import { homedir } from 'os'

export const NAME = 'ei'
export const DIR_NAME = '.easy-install'
export const CLI_DIR = join(__dirname, DIR_NAME).replaceAll('\\', '/')
export const EI_DIR = join(homedir(), DIR_NAME).replaceAll('\\', '/')

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

export async function downloadBinary(url: string) {
  const response = await fetch(url, getFetchOption())
  return await response.arrayBuffer()
}


export function getBinName(bin: string) {
  return process.platform === 'win32' && !bin.endsWith('.exe') &&
    !bin.includes('.')
    ? `${bin}.exe`
    : bin
}

const FilenameMap: Record<string, string> = {
  'aarch64-apple-darwin': `ei-aarch64-apple-darwin.tar.gz`,
  'x86_64-apple-darwin': `ei-x86_64-apple-darwin.tar.gz`,
  'x86_64-pc-windows-gnu': `ei-x86_64-pc-windows-gnu.zip`,
  'x86_64-pc-windows-msvc': `ei-x86_64-pc-windows-msvc.zip`,
  'x86_64-unknown-linux-gnu': `ei-x86_64-unknown-linux-gnu.tar.gz`,
  'x86_64-unknown-linux-musl': `ei-x86_64-unknown-linux-musl.tar.gz`,
}

function getUrl(target: string) {
  const v = FilenameMap[target]
  if (!v) {
    console.log('Not support target: ' + target)
    return
  }
  return `https://github.com/easy-install/easy-install/releases/latest/download/${v}`
}

const EI_BIN_PATH = join(CLI_DIR, getBinName(NAME)).replaceAll('\\', '/')

export async function setupEi() {
  for (const t of detectTargets()) {
    const url = getUrl(t)
    if (!url) {
      continue
    }

    const buffer = new Uint8Array(await downloadBinary(url))
    const fmt = guess(url)
    if (!fmt) {
      continue
    }
    const ret = decode(fmt, buffer)
    if (!ret) {
      continue
    }
    const ei = ret[0].buffer
    const dir = dirname(EI_BIN_PATH)
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true })
    }
    writeFileSync(EI_BIN_PATH, ei)
    return EI_BIN_PATH
  }
}

export async function runEi(
  args = process.argv.slice(2),
  quiet: boolean = false
) {
  if (!existsSync(EI_BIN_PATH)) {
    const r = await setupEi()
    if (!r) {
      throw new Error('Failed to prepare')
    }
  }
  try {
    execFileSync(EI_BIN_PATH, args, { stdio: quiet ? 'ignore' : 'inherit', cwd: __dirname })
  } catch (e) {
    // FIXME: Ignore js errors
  }
}
