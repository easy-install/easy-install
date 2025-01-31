import { expect, test } from 'vitest'
import { fileInstall } from '../ts'
import { existsSync } from 'fs'

test('file install', async () => {
  const url =
    'https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-linux-x86_64'
  const name = 'qjs'
  const output = await fileInstall({ url, name })
  expect(existsSync(output?.installPath!)).toEqual(true)

  const output2 = await fileInstall({ url, name }, undefined, 'test-install')
  expect(existsSync(output2?.installPath!)).toEqual(true)
})
