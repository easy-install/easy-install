import { expect, test } from 'vitest'
import { Repo } from '../ts/repo'
import { isExeUrl, isUrl } from '../ts'

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
      [
        'ahaoboy/ansi2',
        new Repo('ahaoboy', 'ansi2'),
      ],
      [
        'ahaoboy/ansi2@v1',
        new Repo('ahaoboy', 'ansi2', 'v1'),
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
  expect(json!.artifacts['cargo-dist-x86_64-apple-darwin.tar.xz'].name).toEqual(
    'cargo-dist-x86_64-apple-darwin.tar.xz',
  )
})

test('isExeFile', () => {
  for (
    const [a, b] of [
      [
        'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe',
        true,
      ],
      [
        'https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64.exe',
        true,
      ],
      [
        'https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64',
        true,
      ],
      [
        'https://github.com/easy-install/easy-install/releases/download/v0.1.5/ei-x86_64-apple-darwin.tar.gz',
        false,
      ],
      ['https://github.com/easy-install/easy-install', false],
      [
        'https://github.com/easy-install/easy-install/releases/tag/v0.1.5',
        false,
      ],
      [
        'https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64',
        true,
      ],
      [
        'https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64.zip',
        false,
      ],
      [
        'https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64.msi',
        false,
      ],
    ] as const
  ) {
    expect(isExeUrl(a)).toBe(b)
  }
})
