import { readDistManfiest } from '../dist-manifest'
import { downloadJson } from '../download'
import { Repo } from '../repo'
import { isArchiveFile, isDistManfiest, isExeFile, isUrl } from '../tool'
import type { DistManifest, Input, Output } from '../type'
import { artifactInstall } from './artifact'
import { fileInstall } from './file'
import { manifestInstall } from './manifest'
import { repoInstall } from './repo'
import { builtinInstall, getBuiltinName } from './builtin'

export async function install(
  input: Input,
  installDir?: string,
): Promise<Output> {
  const { url, version, name } = input
  const repo = Repo.fromUrl(url)
  if (repo) {
    const name = await getBuiltinName(repo)
    if (name) {
      return builtinInstall(repo, name, installDir)
    }
  }

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

  if (isUrl(url)) {
    if (isArchiveFile(url)) {
      return artifactInstall(url, undefined, installDir)
    }

    if (isExeFile(url)) {
      return fileInstall({ url, name }, url, undefined, installDir)
    }
  }

  if (repo) {
    return repoInstall(repo, name, version, installDir)
  }

  console.log('failed to install', url, version, name)
  return {}
}
