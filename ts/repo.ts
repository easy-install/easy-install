import { downloadJson } from './download'
import { getFetchOption, guessName, isSkip } from './tool'
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
  ): Promise<{ name: string; url: string }[]> {
    const releases = await this.getRelease(tag)
    const v: { name: string; url: string }[] = []
    const filter = new Set()
    for (const { name, url, browser_download_url } of releases.assets || []) {
      if (isSkip(browser_download_url)) {
        continue
      }
      const ret = guessName(browser_download_url)
      console.log(name, ret)
      if (ret && !filter.has(ret.name)) {
        if (bin && bin !== ret.name) {
          continue
        }
        v.push({ name: ret.name, url: browser_download_url })
        filter.add(ret.name)
      }
    }
    if (!v.length) {
      console.log(
        `No asset found`,
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
}
