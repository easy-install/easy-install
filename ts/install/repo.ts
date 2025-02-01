import { getArtifactDownloadUrl } from '../dist-manifest'
import { Repo } from '../repo'

export async function repoInstall(repo: Repo, name?: string, version?: string) {
  const artUrl = repo.getArtifactUrls()
}
