import { join } from 'path'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import {
  displayOutput,
  download,
  endsWithExe,
  getCommonPrefixLen,
  getFilename,
  installOutputFiles,
  isExeFile,
  nameNoExt,
  showSuccess,
} from '../tool'
import { Output, OutputFile } from '../type'
import { manifestInstall } from './manifest'
import { extractTo } from '@easy-install/easy-archive/tool'
import { existsSync, mkdirSync } from 'fs'
import { fileInstall } from './file'
import { getLocalTarget, guessTarget, Os, targetGetOs } from 'guess-target'

export async function repoInstall(
  repo: Repo,
  name?: string,
  version?: string,
  installDir?: string,
) {
  const distUrl = repo.getManfiestUrl()
  const dist = await repo.getManfiest()
  if (dist) {
    return manifestInstall(dist, installDir, distUrl)
  }

  const downloadUrlList = await repo.getAssetUrlList(
    name?.length ? name : undefined,
    version?.length ? version : undefined,
  )

  const output: Output = {}
  for (const i of downloadUrlList) {
    if (!installDir) {
      installDir = getInstallDir()
    }
    if (isExeFile(i)) {
      const out = await fileInstall({ url: i }, i, undefined, installDir)
      Object.assign(output, out)
    } else {
      console.log(`download ${i}`)
      const downloadPath = await download(i)
      const filename = getFilename(downloadPath)
      const { files } = extractTo(downloadPath) || {}
      if (!files) {
        continue
      }
      const list = files.filter((i) => !i.isDir)
      const localTarget = getLocalTarget()
      if (
        endsWithExe(i) && localTarget.some((i) => targetGetOs(i) !== Os.Windows)
      ) {
        continue
      }
      const guess = guessTarget(filename)
      const subDirName = guess.find((i) =>
        localTarget.includes(i.target)
      )?.name ?? nameNoExt(filename)

      if (list.length > 1) {
        installDir = join(installDir, subDirName).replaceAll('\\', '/')
      }
      const prefixLen = getCommonPrefixLen(list.map((i) => i.path))
      if (!existsSync(installDir)) {
        mkdirSync(installDir, { recursive: true })
      }

      const outputFiles: OutputFile[] = []
      for (const { path, mode = 0, isDir, buffer } of list) {
        const installPath = join(installDir, path.slice(prefixLen)).replaceAll(
          '\\',
          '/',
        )
        outputFiles.push({
          mode,
          size: buffer.length,
          isDir: isDir,
          installPath,
          originPath: path,
          buffer,
        })
      }
      installOutputFiles(outputFiles)
      output[i] = {
        installDir,
        files: outputFiles,
      }

      if (Object.keys(output).length) {
        showSuccess()
        console.log(displayOutput(output))
      }
    }
  }

  return output
}
