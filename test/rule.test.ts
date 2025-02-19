import { expect, test } from 'vitest'
import { getRules, matchRules, Rule } from '../ts/rule'

test('rule', () => {
  const rules = getRules()
  for (
    const [a, b, c, d] of [
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
    ]
  ) {
    const { name, rule: { target: { os, arch } } } = matchRules(a, rules)!
    expect(name).toEqual(b)
    expect(os).toEqual(c)
    expect(arch).toEqual(d)
  }
})
