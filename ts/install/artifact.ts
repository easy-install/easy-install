import { join } from 'path'
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
  isArchiveFile,
} from '../tool'
import { DistManifest, Output } from '../type'
import { fileInstall } from './file'
import { existsSync, mkdirSync, readdirSync, statSync } from 'fs'

async function downloadAndInstall(
  url: string,
  dist?: DistManifest,
  dir?: string,
): Promise<undefined | Output> {
  const tmpPath = await downloadToFile(url)
  const tmpDir = await extractTo(tmpPath)

  const getEntry = (p: string) => {
    return join(tmpDir, p)
  }

  const targets = detectTargets()
  const art = dist ? getArtifact(dist, targets) : undefined
  const asset = art ? getAssetsExecutableDir(art) : undefined

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
        const dst = join(installDir, top.replace(prefix + '/', ''))
        const dstDir = dst.split('/').slice(0, -1).join('/')
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
      return {
        downloadUrl: url,
        installDir,
      }
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

        const filename = top.split('/').at(-1)!
        const name = art?.assets?.find((i) =>
          i.path === top
        )?.executable_name ?? filename
        const src = join(tmpDir, name)
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
      return {
        downloadUrl: url,
        installDir,
      }
    }
  }
}
export async function artifactInstall(
  artUrl: string,
  dist?: DistManifest,
  dir?: string,
): Promise<undefined | Output> {
  const v = await getArtifactDownloadUrl(artUrl)
  if (v.length === 0) {
    console.log(`not found download_url for ${artUrl}`)
    return
  }
  if (v.length === 1 && !isArchiveFile(v[0])) {
    console.log(`download ${v[0]}`)
    return await fileInstall({ url: v[0] }, dist, dir)
  }
  for (const url of v) {
    console.log(`download ${url}`)
    return await downloadAndInstall(url, dist, dir)
  }
}
