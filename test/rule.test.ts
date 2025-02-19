import { expect, test } from 'vitest'
import { getRules, matchRules, Rule } from '../ts/rule'

test('rule', () => {
  const rules = getRules()
  for (
    const [a, b, c, d, e] of [
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
    ]
  ) {
    const { name, rule } = matchRules(a, rules)!
    const { target: { os, arch, musl } } = rule
    expect(name).toEqual(b)
    expect(os).toEqual(c)
    expect(arch).toEqual(d)
    expect(!!musl).toEqual(!!e)
  }
})
