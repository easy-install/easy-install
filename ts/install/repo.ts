import { join } from 'path'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import {
  displayOutput,
  download,
  getCommonPrefix,
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
      const filename = downloadPath.split('/').at(-1) ?? downloadPath
      const { files, outputDir } = extractTo(downloadPath) || {}
      if (!files) {
        return {}
      }
      const list = files.filter((i) => !i.isDir)
      if (
        list.length > 1
      ) {
        const name = canInstall(filename)
        if (name) {
          installDir = join(installDir, name).replaceAll('\\', '/')
        }
      }
      const prefixLen = getCommonPrefix(list.map((i) => i.path))?.length ?? 0
      if (!existsSync(installDir)) {
        mkdirSync(installDir, { recursive: true })
      }

      const outputFiles: OutputFile[] = []
      for (const { path, mode = 0, isDir, buffer } of files) {
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
