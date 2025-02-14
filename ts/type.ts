export type Input = {
  url: string
  version?: string
  name: string
}

export type OutputFile = {
  mode: number
  size: number
  installPath: string
  originPath: string
  isDir: boolean
}

export type OutputItem = {
  // downloadUrl: string
  installDir: string
  binDir: string
  files: OutputFile[]
}

export type Output = Record<string, OutputItem>

export type Asset = {
  name: string
  url: string
  browser_download_url: string
}

export type Artifacts = {
  assets: Asset[]
}

export type GhAsset = {
  executable_name?: string
  executable_dir?: string
  name: string
  path: string
  kind?: string
}
export type Artifact = {
  kind?: string
  name: string
  target_triples?: string[]
  assets?: GhAsset[]
}

export type DistManifest = {
  artifacts: Record<string, Artifact>
}
