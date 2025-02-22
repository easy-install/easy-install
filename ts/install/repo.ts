import { join } from 'path'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import {
  displayOutput,
  download,
  getCommonPrefix,
  installFiles,
  isExeFile,
  showSuccess,
} from '../tool'
import { Output, OutputFile } from '../type'
import { manifestInstall } from './manifest'
import { extractTo } from '@easy-install/easy-archive/tool'
import { canInstall } from '../rule'
import { existsSync, mkdirSync } from 'fs'
import { fileInstall } from './file'

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
  if (!installDir) {
    installDir = getInstallDir()
  }
  for (const i of downloadUrlList) {
    if (isExeFile(i)) {
      const out = await fileInstall({ url: i }, i, undefined, installDir)
      Object.assign(output, out)
    } else {
      console.log(`download ${i}`)
      const downloadPath = await download(i)
      const filename = downloadPath.split('/').at(-1)!
      const { files } = extractTo(downloadPath) || {}
      if (!files) {
        return {}
      }
      const list = files.filter((i) => !i.isDir)
      const subDirName = canInstall(filename) ?? filename
      if (list.length > 1) {
        installDir = join(installDir, subDirName).replaceAll('\\', '/')
      }
      const prefixLen = getCommonPrefix(list.map((i) => i.path))?.length ?? 0
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
      installFiles(outputFiles)
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
