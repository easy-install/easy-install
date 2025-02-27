import { runEi } from './ei'

export function install(
  url: string,
  bin: string,
  installDir: string,
  quiet: boolean = false,
) {
  const args = [url, '-d', installDir, '--bin', bin, '--install-only']
  return runEi(args, quiet)
}
