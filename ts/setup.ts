import { CLI_DIR } from './ei'
import { install } from './install'

export function setup(url: string, installDir: string = CLI_DIR) {
  install(url, installDir, true)
}
