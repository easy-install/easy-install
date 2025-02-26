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
  buffer: Uint8Array
}

export type OutputItem = {
  // downloadUrl: string
  installDir: string
  files: OutputFile[]
}

export type Output = Record<string, OutputItem>

export type GhAsset = {
  name: string
  url: string
  browser_download_url: string
}

export type Artifacts = {
  assets: GhAsset[]
}

export type DistAsset = {
  executable_name?: string
  executable_dir?: string
  name: string
  path: string
  kind?: string
}
export type DistArtifact = {
  kind?: string
  name: string
  target_triples?: string[]
  assets?: DistAsset[]
}

export type DistManifest = {
  artifacts: Record<string, DistArtifact>
}
