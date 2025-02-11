import { getArtifactUrlFromManfiest } from '../dist-manifest'
import { DistManifest, Output } from '../type'
import { artifactInstall } from './artifact'

export async function manifestInstall(
  dist: DistManifest,
  dir?: string,
  manifestUrl?: string,
): Promise<Output> {
  const v = await getArtifactUrlFromManfiest(dist, manifestUrl)
  if (!v.length) {
    console.log('failed to install from manifest')
    return {}
  }
  const outptu: Output = {}
  for (const url of v) {
    Object.assign(outptu, await artifactInstall(url, dist, dir))
  }
  return outptu
}
