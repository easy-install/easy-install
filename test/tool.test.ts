import { expect, test } from 'vitest'
import {
  getCommonPrefixLen,
  getFilename,
  isArchiveFile,
  isExeUrl,
} from '../ts/tool'
import { downloadDistManfiest } from '../ts/download'
import { getArtifactUrlFromManfiest } from '../ts/dist-manifest'

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
    expect(isExeUrl(a)).toEqual(b)
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
      [
        ['mujs.dll', 'mujs.exe'],
        0,
      ],
    ] as const
  ) {
    const n = getCommonPrefixLen(a)
    expect(n).toEqual(b)
  }
})

test('getFilename', () => {
  for (
    const [a, b] of [
      ['/a/b', 'b'],
      ['/a/b/c.exe', 'c.exe'],
      ['a', 'a'],
    ]
  ) {
    expect(getFilename(a)).toEqual(b)
  }
})
