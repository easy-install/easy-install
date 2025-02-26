import { existsSync, readFileSync } from 'fs'
import { Repo } from './repo'
import {
  detectTargets,
  getFilename,
  isHashFile,
  isMsiFile,
  isUrl,
  matchTargets,
  removePostfix,
  replaceFilename,
} from './tool'
import { DistArtifact, DistManifest } from './type'
import { getLocalTarget, guessTarget } from 'guess-target'

export function getAssetsExecutableDir(art: DistArtifact) {
  return art.assets?.find((i) => i.kind === 'executable-dir')
}

export function getArtifact(
  dist: DistManifest,
  targets: string[],
): DistArtifact | undefined {
  for (const art of Object.values(dist.artifacts)) {
    if (
      matchTargets(art.target_triples ?? [], targets) &&
      (art.kind || 'executable-zip') === 'executable-zip'
    ) {
      return art
    }
  }
}

export function getAssetByPath(art: DistArtifact, path: string) {
  return art.assets?.find((i) => i.path === path)
}

export function getArtifactUrlFromManfiest(
  dist: DistManifest,
  url?: string,
): string[] {
  const targets = detectTargets()
  const v: string[] = []
  const filter: string[] = []
  for (const key in dist.artifacts) {
    const filename = getFilename(key)
    if (isHashFile(filename) || isMsiFile(filename)) {
      continue
    }
    const art = dist.artifacts[key]

    const localTarget = getLocalTarget()
    const guess = guessTarget(filename)
    const name = guess.find((i) => localTarget.includes(i.target))?.name

    if (name && !filter.includes(name)) {
      if (!isUrl(key) && url) {
        v.push(replaceFilename(url, key))
      } else {
        v.push(key)
      }
      filter.push(name)
      continue
    }

    if (
      matchTargets(art.target_triples ?? [], targets)
    ) {
      if (art.kind && !['executable-zip'].includes(art.kind)) {
        continue
      }
      if (!isUrl(key) && url) {
        v.push(replaceFilename(url, key))
      } else {
        v.push(key)
      }
    }
  }

  return v
}

export function readDistManfiest(path: string): DistManifest | undefined {
  if (!existsSync(path)) {
    return
  }
  const s = readFileSync(path, 'utf-8')
  try {
    return JSON.parse(s)
  } catch (_) {
  }
}
