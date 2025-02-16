import { join } from 'path'
import { downloadBinary, downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  addExecutePermission,
  displayOutput,
  getBinName,
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
  const mode = 0o755
  const originPath = downloadUrl.split('/').at(-1)!
  const isDir = false
  if (!dist) {
    const installPath = join(installDir, getBinName(filename)).replaceAll(
      '\\',
      '/',
    )

    console.log(`download ${downloadUrl}`)
    await downloadToFile(downloadUrl, installPath)
    chmodSync(installPath, mode)
    const size = readFileSync(installPath).length
    const files = [{
      mode,
      size,
      isDir,
      originPath,
      downloadUrl,
      installPath,
      installDir,
    }]
    const output: Output = {
      [downloadUrl]: {
        installDir,
        binDir: installDir,
        files,
      },
    }
    showSuccess()
    console.log(displayOutput(output))
    return output
  }
  const artifact = dist?.['artifacts'][url]
  if (artifact) {
    const bin = await downloadBinary(downloadUrl)
    const name = artifact.name ?? filename
    const installPath = join(installDir, getBinName(name)).replaceAll('\\', '/')
    if (!existsSync(installDir)) {
      mkdirSync(installDir, { recursive: true })
    }
    writeFileSync(installPath, new Uint8Array(bin), { mode })
    addExecutePermission(installPath)
    const size = bin.byteLength
    const files = [{
      size,
      mode,
      downloadUrl,
      installPath,
      installDir,
      originPath,
      isDir,
    }]
    const output: Output = {
      [downloadUrl]: {
        installDir,
        binDir: installDir,
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
