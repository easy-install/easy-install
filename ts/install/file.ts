import { join } from 'path'
import { downloadBinary, downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  addExecutePermission,
  displayOutput,
  getBinName,
  nameNoExt,
  showSuccess,
} from '../tool'
import { DistManifest, Output } from '../type'
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'fs'
import { canInstall } from '../rule'

export type FileInstall = {
  url: string
  name?: string
}

export async function fileInstall(
  info: FileInstall,
  downloadUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<Output> {
  let installDir: string = getInstallDir()
  if (dir) {
    if (dir.includes('/') || dir.includes('\\')) {
      installDir = dir
    } else {
      installDir = join(installDir, dir).replaceAll('\\', '/')
    }
  }

  const { url, name } = info
  const filename = name ?? downloadUrl.split('/').at(-1)!
  const matchName = canInstall(filename) ?? nameNoExt(filename)
  const mode = 0o755
  const originPath = downloadUrl.split('/').at(-1)!
  const isDir = false
  if (!dist) {
    const installPath = join(installDir, getBinName(matchName)).replaceAll(
      '\\',
      '/',
    )

    console.log(`download ${downloadUrl}`)
    await downloadToFile(downloadUrl, installPath)
    chmodSync(installPath, mode)
    const buffer = readFileSync(installPath)
    const files = [{
      mode,
      size: buffer.length,
      isDir,
      originPath,
      downloadUrl,
      installPath,
      installDir,
      buffer,
    }]
    const output: Output = {
      [downloadUrl]: {
        installDir,
        files,
      },
    }
    showSuccess()
    console.log(displayOutput(output))
    return output
  }
  const artifact = dist?.['artifacts'][url]
  if (artifact) {
    const buffer = new Uint8Array(await downloadBinary(downloadUrl))
    const name = artifact.name ?? matchName
    const installPath = join(installDir, getBinName(name)).replaceAll('\\', '/')
    if (!existsSync(installDir)) {
      mkdirSync(installDir, { recursive: true })
    }
    writeFileSync(installPath, buffer, { mode })
    addExecutePermission(installPath)
    const size = buffer.byteLength
    const files = [{
      size,
      mode,
      downloadUrl,
      installPath,
      installDir,
      originPath,
      isDir,
      buffer,
    }]
    const output: Output = {
      [downloadUrl]: {
        installDir,
        files,
      },
    }
    showSuccess()
    console.log(displayOutput(output))
    return output
  } else {
    console.log(`not found/download artifact for ${url}`)
  }
  return {}
}
