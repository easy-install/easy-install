import { downloadJson, downloadText } from '../download'
import { Repo } from '../repo'
import { DistManifest, Output } from '../type'
import { manifestInstall } from './manifest'
let BUILTIN_REPO: Record<string, string> | undefined
const API =
  `https://github.com/ahaoboy/easy-install/raw/refs/heads/main/builtin.json`
async function getBuiltinRepo(): Promise<Record<string, string> | undefined> {
  if (BUILTIN_REPO) return BUILTIN_REPO
  BUILTIN_REPO = await downloadJson(API)
  return BUILTIN_REPO
}

export async function getBuiltinName(repo: Repo): Promise<string | undefined> {
  const builtin = await getBuiltinRepo()
  if (!builtin) return
  for (const [url, name] of Object.entries(builtin)) {
    const item = Repo.fromUrl(url)
    if (!item) {
      continue
    }
    if (item.name === repo.name && item.owner === item.owner) {
      return name
    }
  }
}
export function getDistUrl(name: string): string {
  return `https://github.com/ahaoboy/easy-install/raw/refs/heads/main/dist-manifest/${name}.json`
}

export async function getDist(
  distUrl: string,
  tag: string = 'latest',
): Promise<DistManifest | undefined> {
  const json = await downloadText(distUrl)
  if (tag === 'latest') {
    return JSON.parse(json)
  }
  const tagJson = json.replaceAll(
    '/releases/latest/download/',
    `/releases/download/${tag}/`,
  )
  return JSON.parse(tagJson)
}

export async function builtinInstall(
  repo: Repo,
  name: string,
  installDir?: string,
): Promise<Output> {
  const distUrl = getDistUrl(name)
  const dist = await getDist(distUrl, repo.tag)
  if (!dist) return {}
  return manifestInstall(dist, installDir, distUrl)
}
