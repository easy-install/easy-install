import { expect, test } from 'vitest'
import { fileInstall, Repo } from '../ts'
import { existsSync } from 'fs'

test('fileInstall', async () => {
  const url =
    'https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-linux-x86_64'
  const name = 'qjs'
  const output = await fileInstall({ url, name }, url)
  const item = Object.values(output)[0].files[0]!
  expect(existsSync(item.installPath!)).toEqual(true)

  const output2 = await fileInstall(
    { url, name },
    url,
    undefined,
    'test-install',
  )
  const item2 = Object.values(output2)[0].files[0]!
  expect(existsSync(item2.installPath!)).toEqual(true)
})

test('install starship ', async () => {
  const url = 'https://github.com/starship/starship'
  const repo = Repo.fromUrl(url)!
  const downloadUrlList = await repo.getAssetUrlList()
  expect(downloadUrlList.length).toEqual(1)
  const s = downloadUrlList[0]
  expect(s.endsWith(process.platform === 'win32' ? '.zip' : '.tar.gz')).toEqual(
    true,
  )
})

test('install deno ', async () => {
  const url = 'https://github.com/denoland/deno'
  const repo = Repo.fromUrl(url)!
  const downloadUrlList = await repo.getAssetUrlList()
  expect(downloadUrlList.length).toEqual(2)
})
