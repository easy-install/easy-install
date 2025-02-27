import { runEi } from './ei'

export function install(
  url: string,
  bin?: string,
  installOnly?: boolean,
  installDir?: string,
  quiet: boolean = false,
) {
  const args = [url]
  if (installDir) {
    args.push('-d', installDir)
  }
  if (bin) {
    args.push('--bin', bin)
  }
  if (installOnly) {
    args.push('--install-only')
  }
  return runEi(args, quiet)
}
