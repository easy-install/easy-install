import { expect, test } from 'vitest'
import { downloadDistManfiest, downloadToFile } from '../ts/download'
import { extractTo } from '../ts'
import { existsSync } from 'fs'
import { join } from 'path'

test('downloadDistManfiest', async () => {
  const json = await downloadDistManfiest(
    'https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json',
  )
  expect(json.artifacts['cargo-dist-x86_64-pc-windows-msvc.zip'].kind).toEqual(
    'executable-zip',
  )
})

test('download mujs', async () => {
  const url =
    'https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz'
  const tmpPath = await downloadToFile(url)
  const tmpDir = await extractTo(tmpPath)
  for (
    const i of [
      'mujs',
      'libmujs.a',
      'mujs.pc',
      'mujs-pp',
    ]
  ) {
    const p = join(tmpDir, i)
    expect(existsSync(p)).toEqual(true)
  }
})
