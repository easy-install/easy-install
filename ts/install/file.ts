import { join } from 'path'
import { downloadBinary, downloadToFile } from '../download'
import { getInstallDir } from '../env'
import { addExecutePermission, getBinName } from '../tool'
import { DistManifest, Output } from '../type'
import { existsSync, mkdirSync, writeFileSync } from 'fs'

export type FileInstall = {
  url: string
  name?: string
}

export async function fileInstall(
  info: FileInstall,
  dist?: DistManifest,
  dir?: string,
): Promise<Output | undefined> {
  let installDir: string = getInstallDir()
  if (dir) {
    if (dir.includes('/') || dir.includes('\\')) {
      installDir = dir
    } else {
      installDir = join(installDir, dir)
    }
  }

  const { url, name } = info
  const filename = name ?? url.split('/').at(-1)!

  if (!dist) {
    const installPath = join(installDir, getBinName(filename))
    await downloadToFile(url, installPath)
    return {
      downloadUrl: url,
      installPath,
      installDir,
    }
  }

  const artifact = dist?.['artifacts'][url]
  if (artifact) {
    const bin = await downloadBinary(url)
    const name = artifact.name ?? filename
    const installPath = join(installDir, getBinName(name))
    if (!existsSync(installDir)) {
      mkdirSync(installDir, { recursive: true })
    }
    writeFileSync(installPath, new Uint8Array(bin))
    addExecutePermission(installPath)
    console.log('Installation Successful')
    console.log([url, installPath.replaceAll('\\', '/')].join(' -> '))
    return {
      downloadUrl: url,
      installPath,
      installDir,
    }
  } else {
    console.log(`not found/download artifact for ${url}`)
  }
}
