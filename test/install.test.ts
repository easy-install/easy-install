import { expect, test } from 'vitest'
import { fileInstall } from '../ts'
import { existsSync } from 'fs'

test('fileInstall', async () => {
  const url =
    'https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-linux-x86_64'
  const name = 'qjs'
  const output2 = await fileInstall(
    url,
    name,
    '.test-install',
  )
  const item2 = Object.values(output2)[0].files[0]!
  expect(existsSync(item2.installPath!)).toEqual(true)
})
