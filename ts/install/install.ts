import { readDistManfiest } from '../dist-manifest'
import { downloadJson } from '../download'
import { Repo } from '../repo'
import { download, extractTo, isArchiveFile, isDistManfiest, isUrl } from '../tool'
import type { DistManifest, Input, Output } from '../type'
import { artifactInstall } from './artifact'
import { manifestInstall } from './manifest'

export async function install(
  input: Input,
  installDir: string,
): Promise<Output | undefined> {
  const { url, version = 'latest', bin } = input
  if (isDistManfiest(url)) {
    const dist: DistManifest | undefined = isUrl(url) ? await downloadJson(url) : readDistManfiest(url)
    if (!dist) {
      console.log("failed to read dist-manifest.json")
      return
    }
    return await manifestInstall(dist, installDir)
  }

  if (isUrl(url) && isArchiveFile(url)) {
    return artifactInstall(url, undefined, installDir)
  }
  const repo = Repo.fromUrl(url)
  if (repo) {
    const downloadUrl = await repo.getAssetUrl(
      bin?.length ? bin : undefined,
      version,
    )
    const downloadPath = await download(downloadUrl)
    await extractTo(downloadPath, installDir)
    return {
      // version,
      installDir,
      downloadUrl,
    }
  }

  console.log('failed to install', url, version, bin)
}
