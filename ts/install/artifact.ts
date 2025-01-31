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
  detectTargets,
  extractTo,
  isArchiveFile,
} from '../tool'
import { DistManifest } from '../type'
import { fileInstall } from './file'
import { readdirSync, statSync } from 'fs'

async function downloadAndInstall(
  url: string,
  dist?: DistManifest,
  dir?: string,
) {
  const tmpPath = await downloadToFile(url)
  const tmpDir = await extractTo(tmpPath)

  const getEntry = (p: string) => {
    return join(tmpDir, p)
  }

  const targets = detectTargets()
  const art = dist ? getArtifact(dist, targets) : undefined
  const exeDir = art ? getAssetsExecutableDir(art) : undefined

  if (exeDir && art) {
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
        for (const i of readdirSync(top)) {
          const next = join(top, i).replaceAll('\\', '/')
          q.push(next)
        }
      }

      if (v.length) {
        console.log('No files installed')
      } else {
        console.log('Installation Successful')
        console.log(v.join('\n'))
      }
    }
  }
}
export async function artifactInstall(
  artUrl: string,
  dist?: DistManifest,
  dir?: string,
) {
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
    await downloadAndInstall(url, dist, dir)
  }
}
