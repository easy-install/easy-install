import { downloadJson } from './download'
import { getRules, matchRules, Rule } from './rule'
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
        /https:\/\/github\.com\/([^\/]+)\/([^\/]+)\/releases\/download\/([^\/]+)\/(.+)/,
        /https?:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\/tag\/([^\/]+)/,
        /https:\/\/github\.com\/([^\/]+)\/([^\/]+)/,
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
    const rules = getRules(bin)
    // const rule = new Rule(bin, os, arch, musl)
    if (bin) {
      for (const a of releases.assets) {
        const ret = matchRules(a.name, rules)
        if (
          ret && ret.rule.target.os === os &&
          ret.rule.target.arch === arch &&
          ret.rule.target.musl === musl
        ) {
          return [a.browser_download_url]
        }
      }
    }
    const v: string[] = []
    for (const { name, url, browser_download_url } of releases.assets) {
      if (
        isHashFile(url) || isHashFile(name) ||
        isMsiFile(browser_download_url) || isMsiFile(name)
      ) {
        continue
      }
      const ret = matchRules(name, rules)
      if (
        ret && ret.rule.target.arch === arch &&
        ret.rule.target.os === os &&
        ret.rule.target.musl === musl &&
        !v.includes(browser_download_url)
      ) {
        v.push(browser_download_url)
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

  async matchArtifactUrl(pattern: string): Promise<string[]> {
    const v: string[] = []
    const api = this.getArtifactApi()
    const art = await downloadJson<Artifacts>(api)
    const re = new RegExp(pattern)
    const patternName = pattern.split('/').at(-1)
    const nameRe = patternName && new RegExp(patternName)
    for (const asset of art?.assets || []) {
      if (
        !isHashFile(asset.browser_download_url) &&
        (re.test(asset.browser_download_url) ||
          (nameRe && nameRe.test(asset.name)))
      ) {
        v.push(asset.browser_download_url)
      }
    }
    return v
  }
}
