import { readDistManfiest } from '../dist-manifest'
import { downloadJson } from '../download'
import { Repo } from '../repo'
import { isArchiveFile, isDistManfiest, isUrl } from '../tool'
import type { DistManifest, Input, Output } from '../type'
import { artifactInstall } from './artifact'
import { manifestInstall } from './manifest'
import { repoInstall } from './repo'

export async function install(
  input: Input,
  installDir?: string,
): Promise<Output> {
  const { url, version, name } = input
  if (isDistManfiest(url)) {
    const dist: DistManifest | undefined = isUrl(url)
      ? await downloadJson(url)
      : readDistManfiest(url)
    if (!dist) {
      console.log('failed to read dist-manifest.json')
      return {}
    }
    return await manifestInstall(dist, installDir)
  }

  if (isUrl(url) && isArchiveFile(url)) {
    return artifactInstall(url, undefined, installDir)
  }
  const repo = Repo.fromUrl(url)

  if (repo) {
    return repoInstall(repo, name, version, installDir)
  }

  console.log('failed to install', url, version, name)
  return {}
}
