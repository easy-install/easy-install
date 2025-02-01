import { join, basename } from 'path'
import {
  getArtifact,
  getArtifactDownloadUrl,
  getAssetsExecutableDir,
  hasFile,
} from '../dist-manifest'
import { downloadToFile } from '../download'
import { getInstallDir } from '../env'
import {
  addExecutePermission,
  atomiInstall,
  cleanPath,
  detectTargets,
  extractTo,
  getBinName,
  isArchiveFile,
} from '../tool'
import { DistManifest, Output } from '../type'
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

  const tmpPath = await downloadToFile(downloadUrl)
  const tmpDir = await extractTo(tmpPath)

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
    const v: string[] = []
    while (q.length) {
      const top = q.shift()!
      const entry = getEntry(top)
      const info = statSync(entry)
      if (info.isFile()) {
        const src = join(tmpDir, top)
        const dst = join(installDir, top.replaceAll(
          '\\',
          '/',
        ).replace(prefix + '/', ''))
        const dstDir = basename(dst)
        if (!existsSync(dstDir)) {
          mkdirSync(dstDir, { recursive: true })
        }

        atomiInstall(src, dst)
        addExecutePermission(dst)
        v.push([top, dst].join(' -> '))
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
    } else {
      console.log('Installation Successful')
      console.log(v.join('\n'))
      if (asset.executable_dir) {
        installDir = join(installDir, asset.executable_dir)
      }
      return [{
        downloadUrl,
        installDir,
      }]
    }
  } else {
    let installDir = getInstallDir()
    if (dir) {
      if (dir.includes('\\') || dir.includes('/')) {
        installDir = dir
      } else {
        installDir = join(installDir, dir)
      }
    }
    const v: string[] = []
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
        if (!asset) {
          continue
        }
        let name = filename
        if (asset.name) {
          name = getBinName(asset.name)
        }
        const src = join(tmpDir, asset.path)
        const dst = join(installDir, name)
        atomiInstall(src, dst)
        addExecutePermission(dst)
        v.push([top, dst].join(' -> ').replaceAll('\\', '/'))
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
    } else {
      console.log('Installation Successful')
      console.log(v.join('\n'))
      return [{
        downloadUrl,
        installDir,
      }]
    }
  }
  return []
}
export async function artifactInstall(
  artUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<Output> {
  const v = await getArtifactDownloadUrl(artUrl)
  if (v.length === 0) {
    console.log(`not found download_url for ${artUrl}`)
    return []
  }
  for (const downloadUrl of v) {
    console.log(`download ${downloadUrl}`)
    return isArchiveFile(downloadUrl)
      ? await downloadAndInstall(artUrl, downloadUrl, dist, dir)
      : await fileInstall({ url: artUrl }, downloadUrl, dist, dir)
  }

  return []
}
