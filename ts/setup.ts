import type { DistManifest, Input, Output } from './type'
import { install } from './install'
import { CLI_DIR } from './env'
import { manifestInstall } from './install/manifest'

export async function setup(
  input: Input,
  installDir = CLI_DIR,
): Promise<Output | undefined> {
  return install(input, installDir)
}

export async function setupManifest(
  manifest: DistManifest,
  installDir = CLI_DIR,
) {
  return manifestInstall(manifest, installDir)
}
