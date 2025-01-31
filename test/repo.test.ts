import { expect, test } from 'vitest'
import { Repo } from '../ts/repo'
import { isUrl } from '../ts'

test('getReleasesApiUrl', () => {
  const repo = Repo.fromUrl('https://github.com/denoland/deno')!
  expect(repo.name).toBe('deno')
  expect(repo.owner).toBe('denoland')
  expect(repo.getReleasesApiUrl()).toBe(
    'https://api.github.com/repos/denoland/deno/releases/latest',
  )
  expect(repo.getReleasesApiUrl('v2.1.6')).toBe(
    'https://api.github.com/repos/denoland/deno/releases/tags/v2.1.6',
  )
})

test('getAssetUrl', async () => {
  const repo = Repo.fromUrl('https://github.com/denoland/deno')!
  expect(await repo.getAssetUrl('deno', 'v2.1.6', 'win32', 'x64')).toEqual(
    'https://github.com/denoland/deno/releases/download/v2.1.6/deno-x86_64-pc-windows-msvc.zip',
  )
  expect(await repo.getAssetUrl('deno', 'v2.1.6', 'darwin', 'x64')).toEqual(
    'https://github.com/denoland/deno/releases/download/v2.1.6/deno-x86_64-apple-darwin.zip',
  )
})

test('fromUrl', async () => {
  for (
    const [url, repo] of [
      ['https://github.com/ahaoboy/ansi2', new Repo('ahaoboy', 'ansi2')],
      ['https://api.github.com/repos/ahaoboy/ansi2/releases/latest', undefined],
      [
        'https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11',
        new Repo('ahaoboy', 'ansi2', 'v0.2.11'),
      ],
      [
        'https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz',
        new Repo('ahaoboy', 'ansi2', 'v0.2.11'),
      ],
      [
        'https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip',
        new Repo('ahaoboy', 'ansi2', 'v0.2.11'),
      ],
      [
        'https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip',
        new Repo('Ryubing', 'Ryujinx', '1.2.78'),
      ],
    ] as const
  ) {
    expect(Repo.fromUrl(url)).toEqual(repo)
  }
})

test('isUrl', async () => {
  for (
    const [url, ty] of [
      ['https://github.com/ahaoboy/ansi2', true],
      ['ansi2', false],
      ['./pnpm.json', false],
    ] as const
  ) {
    expect(isUrl(url)).toEqual(ty)
  }
})

test('getArtifactUrls', async () => {
  const repo = Repo.fromUrl(
    'https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.4',
  )!
  const urls = await repo.getArtifactUrls()
  expect(urls.length > 0).toEqual(true)
  if (process.platform === 'win32') {
    expect(urls).toEqual([
      'https://github.com/ahaoboy/mujs-build/releases/download/v0.0.4/mujs-x86_64-pc-windows-gnu.tar.xz',
    ])
  }
})

test('getArtifactApi', async () => {
  const url = 'https://github.com/axodotdev/cargo-dist'
  const repo = Repo.fromUrl(url)!
  expect(repo.getArtifactApi()).toEqual(
    'https://api.github.com/repos/axodotdev/cargo-dist/releases/latest',
  )
})

test('getManfiestUrl', async () => {
  const url = 'https://github.com/axodotdev/cargo-dist'
  const repo = Repo.fromUrl(url)!
  expect(repo.getManfiestUrl()).toEqual(
    'https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json',
  )
})

test('getManfiestUrl', async () => {
  for (
    const [url, json] of [
      [
        'https://github.com/axodotdev/cargo-dist/releases/tag/v0.25.1',
        'https://github.com/axodotdev/cargo-dist/releases/download/v0.25.1/dist-manifest.json',
      ],
      [
        'https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.2',
        'https://github.com/ahaoboy/mujs-build/releases/download/v0.0.2/dist-manifest.json',
      ],
    ] as const
  ) {
    const repo = Repo.fromUrl(url)!
    expect(repo.getManfiestUrl()).toEqual(json)
  }
})

test('getManfiest', async () => {
  const url = 'https://github.com/axodotdev/cargo-dist/releases'
  const repo = Repo.fromUrl(url)!
  expect(repo.getManfiestUrl()).toEqual(
    'https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json',
  )
  const json = await repo.getManfiest()
  expect(json.artifacts['cargo-dist-x86_64-apple-darwin.tar.xz'].name).toEqual(
    'cargo-dist-x86_64-apple-darwin.tar.xz',
  )
})
