import { join } from 'path'
import { downloadBinary } from '../download'
import { getInstallDir } from '../env'
import {
  displayOutput,
  endsWithExe,
  getBinName,
  getFilename,
  guessName,
  installOutputFiles,
  nameNoExt,
  showSuccess,
} from '../tool'
import { Output } from '../type'
import { getLocalTarget, Os, targetGetOs } from 'guess-target'
import { DefaultMode } from '../const'

export type FileInstall = {
  url: string
  name?: string
}

export async function fileInstall(
  downloadUrl: string,
  name?: string,
  dir?: string,
): Promise<Output> {
  let installDir: string = getInstallDir()
  if (dir) {
    if (dir.includes('/') || dir.includes('\\')) {
      installDir = dir
    } else {
      installDir = join(installDir, dir).replaceAll('\\', '/')
    }
  }
  const filename = name ?? getFilename(downloadUrl)
  const localTarget = getLocalTarget()
  if (
    endsWithExe(downloadUrl) &&
    localTarget.some((i) => targetGetOs(i) !== Os.Windows)
  ) return {}
  const binName = name ?? guessName(nameNoExt(filename))?.name ??
    nameNoExt(filename)
  const originPath = downloadUrl.split('/').at(-1)!
  const isDir = false
  const buffer = new Uint8Array(await downloadBinary(downloadUrl))
  const installPath = join(installDir, getBinName(binName)).replaceAll(
    '\\',
    '/',
  )
  const size = buffer.byteLength
  const files = [{
    size,
    mode: DefaultMode,
    downloadUrl,
    installPath,
    installDir,
    originPath,
    isDir,
    buffer,
  }]
  const output: Output = {
    [downloadUrl]: {
      installDir,
      files,
    },
  }
  installOutputFiles(files)
  showSuccess()
  console.log(displayOutput(output))
  return output
}
