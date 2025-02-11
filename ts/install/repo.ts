import { join } from 'path'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import { displayOutput, download, extractTo, showSuccess } from '../tool'
import { Output, OutputItem } from '../type'
import { manifestInstall } from './manifest'

export async function repoInstall(
  repo: Repo,
  name?: string,
  version?: string,
  installDir?: string,
) {
  const distUrl = repo.getManfiestUrl()
  const dist = await repo.getManfiest()
  if (dist) {
    return manifestInstall(dist, installDir, distUrl)
  }

  const downloadUrlList = await repo.getAssetUrlList(
    name?.length ? name : undefined,
    version?.length ? version : undefined,
  )

  const output: Output = {}
  if (!installDir) {
    installDir = getInstallDir()
  }
  for (const i of downloadUrlList) {
    console.log(`download ${i}`)
    const downloadPath = await download(i)
    const files = extractTo(downloadPath, installDir).files

    if (!files) {
      console.log(`failed to install from ${repo.getReleasesUrl()}`)
      return {}
    }
    const v: OutputItem[] = []
    for (const originPath of files.keys()) {
      const installPath = join(installDir, originPath).replaceAll('\\', '/')
      const file = files.get(originPath)!
      const { mode = 0, buffer } = file
      v.push({
        mode,
        size: buffer.length,
        isDir: file.isDir(),
        installDir,
        downloadUrl: i,
        installPath,
        originPath,
      })
    }
    output[i] = v
  }
  showSuccess()
  console.log(displayOutput(output))
  return output
}
