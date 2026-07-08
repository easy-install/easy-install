import { runEi } from './ei'

export function install(
  url: string,
  bin?: string,
  noPath?: boolean,
  installDir?: string,
  quiet: boolean = false,
) {
  const args = [url]
  if (installDir) {
    args.push('-d', installDir)
  }
  if (bin) {
    args.push('--alias', bin)
  }
  if (noPath) {
    args.push('--no-path')
  }
  return runEi(args, quiet)
}
