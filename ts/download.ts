import { tmpdir } from 'os'
import path from 'path'
import fs from 'fs'
import { DistManifest } from './type'
import { getFetchOption, randomId } from './tool'

export async function downloadJson<T>(url: string): Promise<T | undefined> {
  const response = await fetch(url, getFetchOption())
  try {
    return await response.json()
  } catch (_) {
  }
}

export async function downloadText(url: string) {
  const response = await fetch(url, getFetchOption())
  return await response.text()
}

export async function downloadBinary(url: string) {
  const response = await fetch(url, getFetchOption())
  return await response.arrayBuffer()
}

export async function downloadToFile(url: string, outputPath?: string) {
  if (!outputPath) {
    const name = url.split('/').at(-1)!
    const dir = path.join(tmpdir(), randomId())
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true })
    }
    outputPath = path.join(dir, name)
  }
  outputPath = outputPath.replaceAll('\\', '/')
  const dir = outputPath.split('/').slice(0, -1).join('/')
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true })
  }
  const response = await fetch(url, getFetchOption())
  const buf = await response.arrayBuffer()
  fs.writeFileSync(outputPath, Buffer.from(buf))
  return outputPath
}

export async function downloadDistManfiest(
  url: string,
): Promise<DistManifest | undefined> {
  return downloadJson(url)
}
