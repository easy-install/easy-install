import { expect, test } from 'vitest'
import { fileInstall } from '../ts'
import { existsSync } from 'fs'
import { artifactInstall } from '../ts/install/artifact'
import { join } from 'path'

test('fileInstall', async () => {
  const url =
    'https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-linux-x86_64'
  const name = 'qjs'
  const output = await fileInstall({ url, name })
  expect(existsSync(output[0]?.installPath!)).toEqual(true)

  const output2 = await fileInstall({ url, name }, undefined, 'test-install')
  expect(existsSync(output2[0]?.installPath!)).toEqual(true)
})

test('artifactInstall', async () => {
  const url =
    'https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz'
  const output = await artifactInstall(url)!
  const mujsPath = join(output[0]?.installDir!, 'mujs')
  expect(existsSync(mujsPath)).toEqual(true)
})

test('artifactInstall ', async () => {
  const url =
    'https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz'
  const output = await artifactInstall(url)!
  const mujsPath = join(output[0]?.installDir!, 'mujs')
  expect(existsSync(mujsPath)).toEqual(true)
})
