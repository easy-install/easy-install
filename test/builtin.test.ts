import { expect, test } from 'vitest'
import { getBinName, getBuiltinName, getDist, getDistUrl, Repo } from '../ts'

test('getBuiltinName', async () => {
  for (
    const [a, b] of [
      [
        'https://github.com/pnpm/pnpm',
        'pnpm',
      ],
      ['https://github.com/pnpm/pnpm/releases/tag/v10.4.1', 'pnpm'],
      [
        'https://github.com/pnpm/pnpm/releases',
        'pnpm',
      ],
    ]
  ) {
    expect(getBuiltinName(Repo.fromUrl(a)!)).toEqual(b)
  }
})

test('getDist', async () => {
  const distUrl = getDistUrl('pnpm')
  const dist = (await getDist(distUrl))!
  expect(
    Object.keys(dist.artifacts).includes(
      'https://github.com/pnpm/pnpm/releases/latest/download/pnpm-linux-x64',
    ),
  ).toEqual(true)
  const repo = Repo.fromUrl('https://github.com/pnpm/pnpm/releases/tag/v9.15.3')
  const distV9 = (await getDist(distUrl, repo?.tag))!
  expect(
    Object.keys(distV9.artifacts).includes(
      'https://github.com/pnpm/pnpm/releases/download/v9.15.3/pnpm-linux-x64',
    ),
  ).toEqual(true)
})
