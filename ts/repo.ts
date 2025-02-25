import { getLocalTarget, guessTarget } from 'guess-target'
import { downloadJson } from './download'
import {
  detectTargets,
  getFetchOption,
  isArchiveFile,
  isHashFile,
  isMsiFile,
  isMusl,
} from './tool'
import { Artifacts } from './type'
import { DistManifest } from './type'

export class Repo {
  constructor(
    public owner: string,
    public name: string,
    public tag: string | undefined = undefined,
  ) {
  }
  getReleasesUrl() {
    if (this.tag === 'latest' || this.tag === undefined) {
      return `https://github.com/${this.owner}/${this.name}/releases/latest`
    }
    return `https://github.com/${this.owner}/${this.name}/releases/tag/${this.tag}`
  }
  static fromUrl(url: string): Repo | undefined {
    for (
      const re of [
        /^https:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/download\/([^\/]+)\/(.+)$/,
        /^https?:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\/tag\/([^\/]+)$/,
        /^https:\/\/github\.com\/([^\/]+)\/([^\/]+)/,
        /^([^\/@]+)\/([^\/@]+)(?:@([^\/]+))?$/,
      ]
    ) {
      const match = url.match(re)
      if (match) {
        return new Repo(match[1], match[2], match[3])
      }
    }
    return undefined
  }

  getReleasesApiUrl(tag = 'latest') {
    if (tag === 'latest' || tag === undefined) {
      return `https://api.github.com/repos/${this.owner}/${this.name}/releases/latest`
    }
    return `https://api.github.com/repos/${this.owner}/${this.name}/releases/tags/${tag}`
  }

  async getRelease(tag = 'latest'): Promise<Artifacts> {
    const url = this.getReleasesApiUrl(tag)
    const json = await fetch(url, getFetchOption()).then((res) => res.json())
    return json as Artifacts
  }

  async getAssetUrlList(
    bin?: string,
    tag = 'latest',
    os = process.platform,
    arch = process.arch,
    musl = isMusl(),
  ): Promise<string[]> {
    const releases = await this.getRelease(tag)
    // const rule = new Rule(bin, os, arch, musl)
    const localTarget = getLocalTarget()
    if (bin) {
      for (const a of releases.assets) {
        const guess = guessTarget(a.name)
        const name = guess.find((i) => localTarget.includes(i.target))?.name
        if (
          name
        ) {
          return [a.browser_download_url]
        }
      }
    }
    const v: string[] = []
    const filter = new Set()
    for (const { name, url, browser_download_url } of releases.assets) {
      if (
        isHashFile(url) || isHashFile(name) ||
        isMsiFile(browser_download_url) || isMsiFile(name)
      ) {
        continue
      }

      const guess = guessTarget(name)
      const ret = guess.find((i) => localTarget.includes(i.target))
      if (
        ret && !v.includes(browser_download_url) && filter.has(ret.target)
      ) {
        v.push(browser_download_url)
        filter.add(ret.target)
      }
    }
    if (!v.length) {
      console.log(
        `No asset found for bin:${bin} tag:${tag} os:${os} arch:${arch}`,
      )
      return []
    }
    return v
  }

  getArtifactApi(): string {
    return this.tag
      ? `https://api.github.com/repos/${this.owner}/${this.name}/releases/tags/${this.tag}`
      : `https://api.github.com/repos/${this.owner}/${this.name}/releases/latest`
  }

  getManfiestUrl(): string {
    return this.tag
      ? `https://github.com/${this.owner}/${this.name}/releases/download/${this.tag}/dist-manifest.json`
      : `https://github.com/${this.owner}/${this.name}/releases/latest/download/dist-manifest.json`
  }

  async getManfiest(): Promise<DistManifest | undefined> {
    try {
      return await downloadJson(this.getManfiestUrl())
    } finally {
    }
  }

  async getArtifactUrls(): Promise<string[]> {
    const api = this.getArtifactApi()
    const artifacts = await downloadJson<Artifacts>(api)
    const target = detectTargets()
    const v: string[] = []
    const filter: string[] = []
    for (const i of artifacts?.assets || []) {
      for (const pat of target) {
        const remove_target = i.name.replace(pat, '')
        if (
          i.name.includes(pat) &&
          isArchiveFile(i.name) && !filter.includes(remove_target)
        ) {
          v.push(i.browser_download_url)
          filter.push(remove_target)
        }
      }
    }
    return v
  }
}
