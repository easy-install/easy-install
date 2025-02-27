import { CLI_DIR } from './ei'
import { install } from './install'

export function setup(url: string, bin: string, installDir: string = CLI_DIR) {
  install(url, bin, true, installDir, true)
}
