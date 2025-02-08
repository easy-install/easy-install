import { join } from 'path'
import { getArtifactDownloadUrl } from '../dist-manifest'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import { download, extractTo } from '../tool'
import { Output } from '../type'
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
    version,
  )

  const v: Output = []
  if (!installDir) {
    installDir = getInstallDir()
  }
  for (const i of downloadUrlList) {
    console.log(`download ${i}`)
    const downloadPath = await download(i)
    const files = extractTo(downloadPath, installDir).files

    if (files) {
      for (const originPath of files.keys()) {
        const installPath = join(installDir, originPath)
        v.push({
          installDir,
          downloadUrl: i,
          installPath,
          originPath,
        })
      }
    } else {
      v.push({
        installDir,
        downloadUrl: i,
      })
    }
  }

  console.log(
    v.map((i) =>
      `${i.originPath ?? i.downloadUrl} -> ${i.installPath ?? i.installDir}`
    ).join('\n'),
  )
  return v
}
