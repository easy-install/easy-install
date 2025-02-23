import { join } from 'path'
// import { getArtifactDownloadUrl } from '../dist-manifest'
import { downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  displayOutput,
  getCommonPrefixLen,
  getFilename,
  installOutputFiles,
  isArchiveFile,
  nameNoExt,
} from '../tool'
import { DistManifest, Output, OutputFile } from '../type'
import { fileInstall } from './file'
import { extractTo } from '@easy-install/easy-archive/tool'
import { matchName } from '../rule'

async function downloadAndInstall(
  downloadUrl: string,
  installDir: string = getInstallDir(),
): Promise<Output> {
  console.log(`download ${downloadUrl}`)
  const tmpPath = await downloadToFile(downloadUrl)
  const { files } = extractTo(tmpPath)!
  const filename = getFilename(downloadUrl)
  const subDirName = matchName(filename) ?? nameNoExt(filename)
  console.log(subDirName,filename)
  const list = files.filter((i) => !i.isDir)

  if (list.length > 1) {
    installDir = join(installDir, subDirName).replaceAll('\\', '/')
  }
  const outputFiles: OutputFile[] = []
  const prefixLen = getCommonPrefixLen(list.map((i) => i.path))

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
  installOutputFiles(outputFiles)
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
  const output = isArchiveFile(artUrl)
    ? await downloadAndInstall(artUrl, dir)
    : await fileInstall({ url: artUrl }, artUrl, dist, dir)

  return output
}
