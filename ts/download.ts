import { tmpdir } from "os"
import path from "path"
import fs from "fs"
import { DistManifest } from "./type"
import { getFetchOption } from "./tool"

export async function downloadJson<T>(url: string): Promise<T> {
  const response = await fetch(url, getFetchOption())
  return await response.json()
}

export async function downloadText(url: string){
  const response = await fetch(url, getFetchOption())
  return await response.text()
}

export async function downloadBinary(url: string) {
  const response = await fetch(url, getFetchOption())
  return await response.arrayBuffer()
}

export async function downloadToFile(url: string, outputPath?: string) {
  if (!outputPath) {
    const name = url.split("/").at(-1)!
    outputPath = path.join(tmpdir(), name)
  }
  const response = await fetch(url, getFetchOption())
  const buf = await response.arrayBuffer()
  fs.writeFileSync(outputPath, Buffer.from(buf))
  return outputPath
}

export async function downloadDistManfiest(url: string): Promise<DistManifest> {
  return downloadJson(url)
}
