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
    await extractTo(downloadPath, installDir)
    v.push({
      installDir,
      downloadUrl: i,
    })
  }

  console.log(v.map((i) => `${i.downloadUrl} -> ${i.installDir}`).join('\n'))
  return v
}
