import { runEi } from './ei'

export function install(url: string, installDir: string, quiet: boolean = false) {
  const args = [url, '-d', installDir]
  // FIXME: add install-only
  return runEi(args, quiet)
}
