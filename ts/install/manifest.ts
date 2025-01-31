import { getArtifactUrlFromManfiest } from '../dist-manifest'
import { DistManifest } from '../type'
import { artifactInstall } from './artifact'

export async function manifestInstall(dist: DistManifest, dir?: string) {
  const v = await getArtifactUrlFromManfiest(dist)
  if (v.length) {
    console.log('manifestInstall failed')
    return undefined
  }
  for (const url of v) {
    await artifactInstall(url, dist, dir)
  }
}
