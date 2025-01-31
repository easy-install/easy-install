import path, { join } from 'path'
import { getBinName } from './tool'
import type { Input, Output } from './type'
import { existsSync } from 'fs'
import { execFileSync } from 'child_process'
import { install } from './install'

const STEAL_CLI_DIR = path.join(__dirname, 'steal-cli')

export async function setup(input: Input): Promise<Output | undefined> {
  return install(input, STEAL_CLI_DIR)
}

export async function run(input: Input, args = process.argv.slice(2)) {
  const binPath = join(STEAL_CLI_DIR, getBinName(input.bin))
  if (!existsSync(binPath)) {
    await setup(input)
  }
  try {
    execFileSync(binPath, args, { stdio: 'inherit' })
  } catch (e) {
    // FIXME: Ignore js errors
  }
}
