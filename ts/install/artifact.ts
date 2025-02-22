import { join } from 'path'
import { getArtifactDownloadUrl } from '../dist-manifest'
import { downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  displayOutput,
  getCommonPrefix,
  installFiles,
  isArchiveFile,
  nameNoExt,
} from '../tool'
import { DistManifest, Output, OutputFile } from '../type'
import { fileInstall } from './file'
import { extractTo } from '@easy-install/easy-archive/tool'
import { canInstall } from '../rule'

async function downloadAndInstall(
  downloadUrl: string,
  installDir: string = getInstallDir(),
): Promise<Output> {
  console.log(`download ${downloadUrl}`)
  const tmpPath = await downloadToFile(downloadUrl)
  const { files } = extractTo(tmpPath)!
  const filename = downloadUrl.split('/').at(-1)!
  const subDirName = canInstall(filename) ?? nameNoExt(filename)
  const list = files.filter((i) => !i.isDir)

  if (list.length > 1) {
    installDir = join(installDir, subDirName).replaceAll('\\', '/')
  }
  const outputFiles: OutputFile[] = []
  const prefixLen = getCommonPrefix(list.map((i) => i.path))?.length ?? 0

  for (const { isDir, mode = 0, buffer, path } of list) {
    const installPath = join(installDir, path.slice(prefixLen))
    outputFiles.push({
      mode: mode,
      size: buffer.length,
      installPath,
      originPath: path,
      isDir: isDir,
      buffer,
    })
  }
  if (!outputFiles.length) {
    return {}
  }
  installFiles(outputFiles)
  const output = {
    [downloadUrl]: {
      installDir,
      files: outputFiles,
    },
  }
  console.log(displayOutput(output))
  return output
}
export async function artifactInstall(
  artUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<Output> {
  const v = await getArtifactDownloadUrl(artUrl)
  if (v.length === 0) {
    console.log(`not found download_url for ${artUrl}`)
    return {}
  }
  for (const downloadUrl of v) {
    const output = isArchiveFile(downloadUrl)
      ? await downloadAndInstall(downloadUrl, dir)
      : await fileInstall({ url: artUrl }, downloadUrl, dist, dir)

    return output
  }

  return {}
}
