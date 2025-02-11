import { basename, dirname, join } from 'path'
import {
  getArtifact,
  getArtifactDownloadUrl,
  getAssetsExecutableDir,
  hasFile,
} from '../dist-manifest'
import { downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  atomiInstall,
  cleanPath,
  detectTargets,
  displayOutput,
  extractTo,
  getBinName,
  isArchiveFile,
} from '../tool'
import { DistManifest, Output, OutputItem } from '../type'
import { fileInstall } from './file'
import { existsSync, mkdirSync, readdirSync, statSync } from 'fs'

async function downloadAndInstall(
  artUrl: string,
  downloadUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<Output> {
  const targets = detectTargets()
  const art = dist
    ? (dist.artifacts[artUrl] || getArtifact(dist, targets))
    : undefined
  const asset = art ? getAssetsExecutableDir(art) : undefined
  console.log(`download ${downloadUrl}`)
  const tmpPath = await downloadToFile(downloadUrl)
  const tmpDir = extractTo(tmpPath).outputDir

  const getEntry = (p: string) => {
    return join(tmpDir, p)
  }

  if (asset && art) {
    let installDir = getInstallDir()
    const targetDir = dir ?? asset.name
    if (targetDir.includes('/') || targetDir.includes('\\')) {
      installDir = targetDir
    } else {
      installDir = join(installDir, targetDir)
    }

    const prefix = asset.path ?? '.'
    const q = [prefix]
    const v: OutputItem[] = []
    while (q.length) {
      const top = q.shift()!
      const entry = getEntry(top)
      const info = statSync(entry)
      if (info.isFile()) {
        const src = join(tmpDir, top)
        const dst = join(
          installDir,
          top.replaceAll(
            '\\',
            '/',
          ).replaceAll(prefix + '/', ''),
        )
        const dstDir = dirname(dst)
        if (!existsSync(dstDir)) {
          mkdirSync(dstDir, { recursive: true })
        }

        atomiInstall(src, dst)
        // addExecutePermission(dst)

        v.push({
          mode: info.mode,
          size: info.size,
          installDir: asset.executable_dir
            ? join(installDir, asset.executable_dir).replaceAll('\\', '/')
            : installDir,
          installPath: dst,
          originPath: top,
          downloadUrl,
          isDir: info.isDirectory(),
        })
      } else if (info.isDirectory()) {
        const curDir = join(tmpDir, top)
        for (const i of readdirSync(curDir)) {
          const next = cleanPath(join(top, i).replaceAll('\\', '/'))
          q.push(next)
        }
      }
    }

    if (!v.length) {
      console.log('No files installed')
      return {}
    }

    const output = {
      [downloadUrl]: v,
    }
    console.log(displayOutput(output))
    return output
  }

  let installDir = getInstallDir()
  if (dir) {
    if (dir.includes('\\') || dir.includes('/')) {
      installDir = dir
    } else {
      installDir = join(installDir, dir)
    }
  }
  const v: OutputItem[] = []
  const q = ['.']
  const allow = (p: string) => !art || hasFile(art, p)
  while (q.length) {
    const top = q.shift()!
    const entry = getEntry(top)
    const info = statSync(entry)
    if (info.isFile()) {
      if (!allow(top)) {
        continue
      }

      const filename = basename(top)
      const asset = art?.assets?.find((i) => i.path === top)
      let name = filename
      if (asset) {
        if (asset.executable_name) {
          name = getBinName(asset.executable_name)
        }
      }
      const src = join(tmpDir, asset?.path ?? top)
      const dst = join(installDir, name).replaceAll('\\', '/')
      atomiInstall(src, dst)
      // addExecutePermission(dst)
      v.push({
        mode: info.mode,
        size: info.size,
        installDir,
        installPath: dst,
        originPath: asset?.path ?? top,
        downloadUrl,
        isDir: info.isDirectory(),
      })
    } else if (info.isDirectory()) {
      const curDir = join(tmpDir, top)
      for (const i of readdirSync(curDir)) {
        const next = cleanPath(join(top, i).replaceAll('\\', '/'))
        q.push(next)
      }
    }
  }
  if (!v.length) {
    console.log('No files installed')
    return {}
  }

  const output = {
    [downloadUrl]: v,
  }
  console.log(displayOutput(output))
  return output
}
export async function artifactInstall(
  artUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<Output> {
  const v = await getArtifactDownloadUrl(artUrl)
  if (v.length === 0) {
    console.log(`not found download_url for ${artUrl}`)
    return {}
  }
  for (const downloadUrl of v) {
    const output = isArchiveFile(downloadUrl)
      ? await downloadAndInstall(artUrl, downloadUrl, dist, dir)
      : await fileInstall({ url: artUrl }, downloadUrl, dist, dir)

    return output
  }

  return {}
}
