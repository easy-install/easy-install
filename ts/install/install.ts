import { readDistManfiest } from '../dist-manifest'
import { downloadJson } from '../download'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import {
  download,
  extractTo,
  isArchiveFile,
  isDistManfiest,
  isUrl,
} from '../tool'
import type { DistManifest, Input, Output } from '../type'
import { artifactInstall } from './artifact'
import { manifestInstall } from './manifest'

export async function install(
  input: Input,
  installDir?: string,
): Promise<Output> {
  const { url, version = 'latest', name: bin } = input
  if (isDistManfiest(url)) {
    const dist: DistManifest | undefined = isUrl(url)
      ? await downloadJson(url)
      : readDistManfiest(url)
    if (!dist) {
      console.log('failed to read dist-manifest.json')
      return []
    }
    return await manifestInstall(dist, installDir)
  }

  if (isUrl(url) && isArchiveFile(url)) {
    return artifactInstall(url, undefined, installDir)
  }
  const repo = Repo.fromUrl(url)
  if (repo) {
    const distUrl = repo.getManfiestUrl()
    const dist = await repo.getManfiest()
    if (dist) {
      return manifestInstall(dist, installDir, distUrl)
    }

    const downloadUrlList = await repo.getAssetUrlList(
      bin?.length ? bin : undefined,
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

  console.log('failed to install', url, version, bin)
  return []
}
