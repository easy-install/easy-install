import { join } from 'path'
import { CLI_DIR, getBinName } from './ei'
import { existsSync } from 'fs'
import { execFileSync } from 'child_process'
import { install } from './install'

export async function run(
  url: string,
  bin: string,
  args = process.argv.slice(2),
  installDir: string = CLI_DIR,
) {
  const binPath = join(installDir, getBinName(bin)).replaceAll('\\', '/')
  if (!existsSync(binPath)) {
    await install(url, bin, true, installDir, true)
  }
  try {
    execFileSync(binPath, args, {
      stdio: 'inherit',
      cwd: process.cwd(),
    })
  } catch (e) {
    // FIXME: Ignore js errors
  }
}
