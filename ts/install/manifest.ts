import { getArtifactUrlFromManfiest } from '../dist-manifest'
import { DistManifest, Output } from '../type'
import { artifactInstall } from './artifact'

export async function manifestInstall(
  dist: DistManifest,
  dir?: string,
  manifestUrl?: string
): Promise<Output> {
  const v = await getArtifactUrlFromManfiest(dist, manifestUrl)
  if (!v.length) {
    console.log('manifestInstall failed')
    return []
  }
  const list: Output = []
  for (const url of v) {
    list.push(...await artifactInstall(url, dist, dir))
  }
  return list
}
