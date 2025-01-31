import * as fs from "fs"
import { execSync } from "child_process"
import { tmpdir } from "os"
import * as path from "path"
import { readFileSync } from "fs"

export function isUrl(s: string): boolean {
  return ['https://', 'http://'].some(i => s.startsWith(i))
}
export const ArchiveFmtList = [
  ".tar",
  ".tbz2",
  ".tar.bz2",
  ".tgz",
  ".tar.gz",
  ".txz",
  ".tar.xz",
  ".tzstd",
  ".tzst",
  ".tar.zst",
  ".zip",
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
    "User-Agent": "GitHub Actions",
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
    const name = url.split("/").at(-1)!
    outputPath = path.join(tmpdir(), name)
  }
  const response = await fetch(url, getFetchOption())
  const buf = await response.arrayBuffer()
  fs.writeFileSync(outputPath, Buffer.from(buf))
  return outputPath
}

export function extractTo(compressedFilePath: string, outputDir?: string) {
  if (!outputDir) {
    outputDir = path.join(tmpdir(), Math.random().toString(36).slice(2))
  }

  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true })
  }
  const rules = [
    {
      ext: [".zip"],
      cmd: process.platform !== 'win32' ? `unzip -o "${compressedFilePath}" -d "${outputDir}"` : `powershell -c "Expand-Archive -Path ${compressedFilePath} -DestinationPath  ${outputDir} -Force"`,
    },
    { ext: [".tar"], cmd: `tar -xf "${compressedFilePath}" -C "${outputDir}"` },
    {
      ext: [".tar.gz", ".tgz"],
      cmd: `tar -xzf "${compressedFilePath}" -C "${outputDir}"`,
    },
    {
      ext: [".tar.bz2"],
      cmd: `tar -xjf "${compressedFilePath}" -C "${outputDir}"`,
    },
    { ext: [".7z"], cmd: `7z x "${compressedFilePath}" -o"${outputDir}"` },
    { ext: [".rar"], cmd: `unrar x "${compressedFilePath}" "${outputDir}"` },
    { ext: [".rar"], cmd: `unrar x "${compressedFilePath}" "${outputDir}"` },
  ] as const

  for (const { ext, cmd } of rules) {
    for (const e of ext) {
      if (compressedFilePath.endsWith(e)) {
        execSync(cmd)
      }
    }
  }
  return outputDir
}

export function getPlatforms(os = process.platform) {
  let platforms: string[]
  switch (os) {
    case "linux":
      platforms = ["unknown-linux-gnu"]
      break
    case "darwin":
      platforms = ["apple-darwin"]
      break
    case "win32":
      platforms = ["pc-windows-msvc", "pc-windows-gnu"]
      break
    default:
      throw new Error(`Unsupported platform ${os}.`)
  }
  return platforms
}

export function detectTargets(
  platform = process.platform,
  arch = process.arch,
  musl = isMusl(),
): string[] {
  switch (platform) {
    case "darwin": {
      switch (arch) {
        case "arm64": {
          return ["aarch64-apple-darwin"]
        }
        case "x64": {
          return ["x86_64-apple-darwin"]
        }
      }
    }
    case "linux": {
      switch (arch) {
        case "arm64": {
          if (musl) {
            return ["aarch64-unknown-linux-musl", "aarch64-unknown-linux-gnu"]
          }
          return ["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"]
        }
        case "x64": {
          if (musl) {
            return ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"]
          }
          return ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]
        }
      }
    }

    case "win32": {
      switch (arch) {
        case "x64": {
          return ["x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu"]
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
  return process.platform === "win32" ? `${bin}.exe` : bin
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
  if (process.platform === "linux") {
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
  f.includes("libc.musl-") || f.includes("ld-musl-")

const isMuslFromFilesystem = () => {
  try {
    return readFileSync("/usr/bin/ldd", "utf-8").includes("musl")
  } catch {
    return false
  }
}

const isMuslFromReport = () => {
  const report: any = typeof process.report.getReport === "function"
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
    return require("child_process")
      .execSync("ldd --version", { encoding: "utf8" })
      .includes("musl")
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