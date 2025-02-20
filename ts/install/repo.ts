import { join } from 'path'
import { getInstallDir } from '../env'
import { Repo } from '../repo'
import { displayOutput, download, isExeFile, showSuccess } from '../tool'
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
      const filename = downloadPath.split('/').at(-1)
      const { files } = extractTo(downloadPath) || {}
      if (
        filename && files &&
        files.keys().filter((i) => !files.get(i)?.isDir).length > 1
      ) {
        const name = canInstall(filename)
        if (name) {
          installDir = join(installDir, name).replaceAll('\\', '/')
        }
      }
      if (!existsSync(installDir)) {
        mkdirSync(installDir, { recursive: true })
      }
      extractTo(downloadPath, installDir)
      if (!files) {
        console.log(`failed to install from ${repo.getReleasesUrl()}`)
        return {}
      }
      const outputFiles: OutputFile[] = []
      for (const originPath of files.keys()) {
        const installPath = join(installDir, originPath).replaceAll('\\', '/')
        const file = files.get(originPath)!
        const { mode = 0, buffer } = file
        outputFiles.push({
          mode,
          size: buffer.length,
          isDir: file.isDir,
          installPath,
          originPath,
        })
      }
      output[i] = {
        installDir,
        binDir: installDir,
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
