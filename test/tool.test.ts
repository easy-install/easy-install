import { expect, test } from 'vitest'
import {
  getAssetNames,
  getCommonPrefixLen,
  isArchiveFile,
  isExeFile,
} from '../ts/tool'

import { downloadDistManfiest } from '../ts/download'
import { Repo } from '../ts'
import {
  getArtifact,
  // getArtifactDownloadUrl,
  getArtifactUrlFromManfiest,
  hasFile,
  readDistManfiest,
} from '../ts/dist-manifest'

test('getAssetNames', () => {
  expect(getAssetNames('deno', 'win32', 'x64')).toEqual([
    'deno-x86_64-pc-windows-msvc',
    'deno-x86_64-pc-windows-gnu',
  ])
  expect(getAssetNames('deno', 'linux', 'x64')).toEqual([
    'deno-x86_64-unknown-linux-gnu',
  ])
  expect(getAssetNames('deno', 'linux', 'x64', true)).toEqual([
    'deno-x86_64-unknown-linux-musl',
  ])
  expect(getAssetNames('deno', 'darwin', 'x64')).toEqual([
    'deno-x86_64-apple-darwin',
  ])
  expect(getAssetNames('deno', 'darwin', 'arm64')).toEqual([
    'deno-aarch64-apple-darwin',
  ])
})

test('isArchiveFile', () => {
  for (
    const [url, ty] of [
      ['https://github.com/ahaoboy/ansi2', false],
      ['https://api.github.com/repos/ahaoboy/ansi2/releases/latest', false],
      ['https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11', false],
      [
        'https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz',
        true,
      ],
      [
        'https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip',
        true,
      ],
    ] as const
  ) {
    expect(isArchiveFile(url)).toEqual(ty)
  }
})

test('manifest_jsc', async () => {
  const repo = new Repo('ahaoboy', 'jsc-build')
  const dist = (await repo.getManfiest())!
  const art = getArtifact(dist, ['x86_64-unknown-linux-gnu'])!
  for (
    const [k, v] of [
      ['bin/jsc', true],
      ['lib/libJavaScriptCore.a', true],
      ['lib/jsc', false],
    ] as const
  ) {
    expect(hasFile(art, k)).toEqual(v)
  }
})

test('manifest_mujs', async () => {
  const repo = new Repo('ahaoboy', 'mujs-build')
  const dist = (await repo.getManfiest())!
  const art = getArtifact(dist, ['x86_64-unknown-linux-gnu'])!
  for (
    const [k, v] of [
      ['mujs', true],
      ['mujs.exe', false],
    ] as const
  ) {
    expect(hasFile(art, k)).toEqual(v)
  }
  const artWin = getArtifact(dist, ['x86_64-pc-windows-gnu'])!
  for (
    const [k, v] of [
      ['mujs', false],
      ['mujs.exe', true],
    ] as const
  ) {
    expect(hasFile(artWin, k)).toEqual(v)
  }
})

test('install_from_manfiest', async () => {
  const url =
    'https://github.com/ahaoboy/mujs-build/releases/latest/download/dist-manifest.json'
  const dist = (await downloadDistManfiest(url))!
  const v = getArtifactUrlFromManfiest(dist, url)
  expect(v.length > 0).toEqual(true)
})

test('cargo_dist', async () => {
  const url =
    'https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json'
  const dist = (await downloadDistManfiest(url))!
  const v = getArtifactUrlFromManfiest(dist, url)
  expect(v.length > 0).toEqual(true)
})

test('deno', async () => {
  const repo = Repo.fromUrl('https://github.com/denoland/deno')!
  const v = await repo.getArtifactUrls()
  expect(v.length > 0).toEqual(true)
})

// test('get_artifact_download_url', async () => {
//   for (
//     const url of [
//       'https://github.com/Ryubing/Ryujinx/releases/latest/download/^ryujinx-*.*.*-win_x64.zip',
//       'https://github.com/Ryubing/Ryujinx/releases/download/1.2.80/ryujinx-*.*.*-win_x64.zip',
//       'https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip',
//       'https://github.com/shinchiro/mpv-winbuild-cmake/releases/latest/download/^mpv-x86_64-v3-.*?-git-.*?',
//       'https://github.com/NickeManarin/ScreenToGif/releases/latest/download/ScreenToGif.[0-9]*.[0-9]*.[0-9]*.Portable.x64.zip',
//       'https://github.com/ip7z/7zip/releases/latest/download/7z.*?-linux-x64.tar.xz',
//       'https://github.com/mpv-easy/mpv-winbuild/releases/latest/download/mpv-x86_64-v3-.*?-git-.*?.zip',
//       'https://github.com/starship/starship',
//     ]
//   ) {
//     const v = await getArtifactDownloadUrl(url)
//     expect(v.length).toEqual(1)
//   }
// })

// test('graaljs', async () => {
//   const path = './dist-manifest/graaljs.json'
//   const dist = readDistManfiest(path)!
//   const v = getArtifactUrlFromManfiest(dist, path)
//   expect(v.length).toEqual(1)
//   for (const i of v) {
//     const url = await getArtifactDownloadUrl(i)
//     expect(url.length).toEqual(1)
//   }
// })

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
    ] as const
  ) {
    expect(isExeFile(a)).toEqual(b)
  }
})

test('getCommonPrefix', () => {
  for (
    const [a, b] of [
      [
        [
          '/a/ab/c',
          '/a/ad',
          '/a/ab/d',
        ],
        3,
      ],
      [
        ['a'],
        0,
      ],
      [
        ['/a'],
        1,
      ],
      [
        ['/a/b'],
        3,
      ],
    ] as const
  ) {
    expect(getCommonPrefixLen(a)).toEqual(b)
  }
})
