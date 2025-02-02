import { existsSync, readFileSync } from 'fs'
import { Repo } from './repo'
import {
  detectTargets,
  isUrl,
  matchTargets,
  removePostfix,
  replaceFilename,
} from './tool'
import { Artifact, DistManifest } from './type'

export function getAssetsExecutableDir(art: Artifact) {
  return art.assets?.find((i) => i.kind === 'executable-dir')
}

export function getArtifact(
  dist: DistManifest,
  targets: string[],
): Artifact | undefined {
  for (const art of Object.values(dist.artifacts)) {
    if (
      matchTargets(art.target_triples ?? [], targets) &&
      (art.kind || 'executable-zip') === 'executable-zip'
    ) {
      return art
    }
  }
}

export function getAssetByPath(art: Artifact, path: string) {
  return art.assets?.find((i) => i.path === path)
}

export function getArtifactUrlFromManfiest(
  dist: DistManifest,
  url?: string,
): string[] {
  const targets = detectTargets()
  const v: string[] = []

  for (const name in dist.artifacts) {
    const art = dist.artifacts[name]
    if (
      matchTargets(art.target_triples ?? [], targets) &&
      ((art.kind ?? 'executable-zip') === 'executable-zip')
    ) {
      if (!isUrl(name) && url) {
        v.push(replaceFilename(url, name))
      } else {
        v.push(name)
      }
    }
  }

  return v
}

export function hasFile(art: Artifact, path: string) {
  path = path.replaceAll('\\', '/')
  if (art.name) {
    const prefix = removePostfix(art.name) + '/'
    if (path.startsWith(prefix)) {
      path = path.slice(prefix.length)
    }
  }

  for (const i of art.assets ?? []) {
    if (path === '*') {
      // FIXME: support regex
      return true
    }

    if (path === i.path) {
      switch (i.kind) {
        case 'executable':
        case 'c_dynamic_library':
        case 'c_static_library': {
          return true
        }
        // case "readme":
        // case "license":
        // case "changelog":
        default: {
          return false
        }
      }
    }
  }

  return false
}

export function isRegex(s: string): boolean {
  return s.includes('*')
}

export async function getArtifactDownloadUrl(
  artUrl: string,
): Promise<string[]> {
  const v: string[] = []
  if (!isRegex(artUrl)) {
    return [artUrl]
  }
  const repo = Repo.fromUrl(artUrl)
  if (repo) {
    return await repo.matchArtifactUrl(artUrl)
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
