import { join } from 'path'
import { detectTargets, getBinName } from './tool'
import type { DistManifest, Input } from './type'
import { existsSync } from 'fs'
import { execFileSync } from 'child_process'
import { CLI_DIR } from './env'
import { setup, setupManifest } from './setup'
import { getArtifact } from './dist-manifest'

export async function run(
  input: Input,
  installDir = CLI_DIR,
  args = process.argv.slice(2),
) {
  const binPath = join(installDir, getBinName(input.name)).replaceAll('\\', '/')
  if (!existsSync(binPath)) {
    await setup(input)
  }
  try {
    execFileSync(binPath, args, { stdio: 'inherit' })
  } catch (e) {
    // FIXME: Ignore js errors
  }
}

export async function runManifest(
  manifest: DistManifest,
  name?: string,
  installDir = CLI_DIR,
  args = process.argv.slice(2),
) {
  const art = getArtifact(manifest, detectTargets())
  if (!art) {
    console.log('not found artifact')
    return
  }
  const binPath = join(installDir, getBinName(name ?? art.name)).replaceAll(
    '\\',
    '/',
  )
  if (!existsSync(binPath)) {
    await setupManifest(manifest, installDir)
  }
  try {
    execFileSync(binPath, args, { stdio: 'inherit' })
  } catch (e) {
    // FIXME: Ignore js errors
  }
}
