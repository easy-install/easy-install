import { expect, test } from 'vitest'
import { matchName } from '../ts/rule'

test('rule', () => {
  for (
    const [a, b, c, d, e] of [
      [
        'mujs-x86_64-unknown-linux-gnu.tar.gz',
        'mujs',
        'linux',
        'x64',
      ],
      ['mujs-x86_64-unknown-linux-gnu.tar.xz', 'mujs', 'linux', 'x64'],
      ['mise-v2025.2.6-linux-x64', 'mise', 'linux', 'x64'],
      ['zig-linux-x86_64-0.13.0.tar.xz', 'zig', 'linux', 'x64'],
      [
        'vmutils-linux-amd64-v1.111.0-enterprise.tar.gz',
        'vmutils',
        'linux',
        'x64',
      ],
      ['boa-linux-amd64', 'boa', 'linux', 'x64'],
      ['boa-macos-amd64', 'boa', 'darwin', 'x64'],
      ['yt-dlp.exe', 'yt-dlp', 'win32', 'x64'],
      ['xst-mac64.zip', 'xst', 'darwin', 'x64'],
      ['xst-mac64arm.zip', 'xst', 'darwin', 'arm64'],
      ['xst-lin64.zip', 'xst', 'linux', 'x64'],
      ['xst-win64.zip', 'xst', 'win32', 'x64'],
      ['ryujinx-1.2.82-linux_arm64.tar.gz', 'ryujinx', 'linux', 'arm64'],
      ['ryujinx-1.2.82-linux_x64.tar.gz', 'ryujinx', 'linux', 'x64'],
      ['ryujinx-1.2.82-win_x64.zip', 'ryujinx', 'win32', 'x64'],
      ['rcedit-x64.exe', 'rcedit', 'win32', 'x64'],
      ['starship-aarch64-pc-windows-msvc.zip', 'starship', 'win32', 'arm64'],
      ['starship-i686-pc-windows-msvc.zip', 'starship', 'win32', 'ia32'],
      ['starship-x86_64-pc-windows-msvc.zip', 'starship', 'win32', 'x64'],
      ['starship-x86_64-unknown-freebsd.tar.gz', 'starship', 'freebsd', 'x64'],
      [
        'starship-arm-unknown-linux-musleabihf.tar.gz',
        'starship',
        'linux',
        'arm',
        'true',
      ],
      [
        'starship-i686-unknown-linux-musl.tar.gz',
        'starship',
        'linux',
        'ia32',
        'true',
      ],
      ['starship-x86_64-unknown-linux-gnu.tar.gz', 'starship', 'linux', 'x64'],
      [
        'starship-x86_64-unknown-linux-musl.tar.gz',
        'starship',
        'linux',
        'x64',
        'true',
      ],

      ['qjs-windows-x86_64.exe', 'qjs', 'win32', 'x64'],
      ['qjs-linux-x86_64', 'qjs', 'linux', 'x64'],
      ['qjs-darwin', 'qjs', 'darwin', 'x64'],
      ['llrt-windows-x64-full-sdk.zip', 'llrt', 'win32', 'x64'],
      ['bun-linux-x64-baseline.zip', 'bun', 'linux', 'x64'],
    ] as const
  ) {
    const name = matchName(a, undefined, c, d, !!e)
    expect(name).toBe(b)
  }
})

test('matchName', () => {
  for (
    const [a, b, c] of [
      ['ryujinx-1.2.82-macos_universal.app.tar.gz', 'darwin', 'x64'],
      ['ryujinx-1.2.82-macos_universal.app.tar.gz', 'darwin', 'arm64'],
      ['ffmpeg-n7.1-latest-win64-gpl-7.1.zip', 'win32', 'x64'],
      ['7z2409-linux-x64.tar.xz', 'linux', 'x64'],
      ['mpy-easy-windows-full.zip', 'win32', 'x64'],
      ['mpy-easy-windows-full.zip', 'win32', 'x64'],
      ['ffmpeg-x86_64-v3-git-5470d024e.zip', 'win32', 'x64'],
      ['mpv-x86_64-v3-20250220-git-f9271fb.zip', 'win32', 'x64'],
      ['mise-v2025.2.7-macos-x64.tar.gz', 'darwin', 'x64'],
      ['7z2409-x64.exe', 'win32', 'x64'],
    ] as const
  ) {
    const name = matchName(a, undefined, b, c, false)
    expect(!!name).toBe(true)
  }
})
