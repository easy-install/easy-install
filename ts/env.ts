import { homedir } from 'os'
import { join } from 'path'

export function getInstallDir() {
  return join(homedir(), '.easy-install')
}

export const CLI_DIR = join(__dirname, '.easy-install')
