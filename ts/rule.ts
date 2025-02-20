import { extensions, Fmt } from '@easy-install/easy-archive'
import { detectTargets, isMusl } from './tool'

export type Target = {
  target: string
  rank: number
  os: string
  arch: string
  musl?: boolean
}

function getExtRegex(): string {
  const v = [
    Fmt.Tar,
    Fmt.TarBz,
    Fmt.TarGz,
    Fmt.TarXz,
    Fmt.TarZstd,
    Fmt.Zip,
  ].map((i) => extensions(i)).flat().join('|').replaceAll('.', '\\.')
  return '(' + v + ')'
}

export type Rule = {
  target: Target
  rule: RegExp
  rank: number
}

const OS_LIST: NodeJS.Platform[] = [
  'darwin',
  'linux',
  'win32',
  'freebsd',
]
const ARCH_LIST: NodeJS.Architecture[] = [
  'x64',
  'arm64',
  'ia32',
  'arm',
]
// TODO: support musl
// const MUSL_LIST: NodeJS.Architecture[] = []
const seqRe = '[_-\\s]'
const versionRe = `v\?(\\d+\\.\\d+\\.\\d+)`
function targetToRules(target: Target, bin?: string): Rule[] {
  const reList: { rule: string; rank: number }[] = []
  const binRe = bin?.length ? `(${bin})` : `([^\/]+)`
  const s = target.target.replaceAll('_', '-')
    .replaceAll('-', seqRe)

  for (
    const [rule, rank] of [
      // name-version-target
      [`^${binRe}${seqRe}${versionRe}${seqRe}${s}`, 10],
      // name-target-version
      [`^${binRe}${seqRe}${s}${seqRe}${versionRe}`, 10],
      // name-target
      [`^${binRe}${seqRe}${s}`, 5],
    ] as const
  ) {
    reList.push({ rule: rule, rank: rank + target.rank })
  }
  const ext = getExtRegex()
  const reExtList = reList.map((i) => ({
    rule: i.rule + ext + '$',
    rank: i.rank + 5,
  }))

  const v: Rule[] = [...reExtList, ...reList].map(({ rank, rule }) => {
    return {
      rule: new RegExp(rule, 'i'),
      target,
      rank,
    }
  })
  return v
}

function getCommonRules(
  bin?: string,
  os = process.platform,
  arch = process.arch,
  musl = isMusl(),
): Rule[] {
  const v: Rule[] = []
  for (const { target, rank } of getCommonTargets(os, arch, musl)) {
    const t: Target = {
      rank,
      target,
      os,
      arch,
      musl,
    }
    for (const r of targetToRules(t, bin)) {
      v.push(r)
    }
  }
  return v
}

export function getRules(bin?: string): Rule[] {
  const v: Rule[] = []
  for (const os of OS_LIST) {
    for (const arch of ARCH_LIST) {
      const MUSL_LIST = [false]
      if (os === 'linux') {
        MUSL_LIST.push(true)
      }

      for (const musl of MUSL_LIST) {
        for (const target of detectTargets(os, arch, musl)) {
          const t: Target = {
            rank: 10,
            target,
            os,
            arch,
            musl,
          }
          for (const r of targetToRules(t, bin)) {
            v.push(r)
          }
        }
      }

      for (const i of getCommonRules(bin, os, arch, false)) {
        v.push(i)
      }
    }
  }

  // windows
  const binRe = bin?.length ? `(${bin})` : `([^\/]+)`
  for (
    const { target, rank } of [
      ...getCommonTargets('win32', 'x64').map((i) => ({
        target: `^${binRe}${seqRe}${
          i.target.replaceAll(
            '_',
            '-',
          ).replaceAll('-', seqRe)
        }.exe$`,
        rank: 10,
      })),
      { target: `^${binRe}${seqRe}${versionRe}.exe$`, rank: 5 },
      { target: `^${binRe}${seqRe}(x86|x64).exe$`, rank: 5 },
      { target: `^${binRe}.exe$`, rank: 1 },
    ]
  ) {
    v.push({
      target: {
        rank: 20,
        os: 'win32',
        arch: 'x64',
        target,
      },
      rank: 20 + rank,
      rule: new RegExp(target, 'i'),
    })
  }

  return v.sort((a, b) => b.rank - a.rank)
}

export function matchRules(
  s: string,
  rules: Rule[],
  // bin?: string,
): { name: string; rule: Rule }[] {
  const v: { name: string; rule: Rule }[] = []
  for (const rule of rules) {
    if (v.length && v[0].rule.rank !== rule.rank) {
      continue
    }
    const name = s.match(rule.rule)?.[1]
    if (name) {
      v.push({ name, rule })
    }
  }
  return v
}

export function canInstall(
  s: string,
  bin?: string,
  os = process.platform,
  arch = process.arch,
  musl = isMusl(),
): string | undefined {
  const rules = getRules(bin)
  const ret = matchRules(s, rules)
  for (const { rule, name } of ret) {
    if (os === 'linux' || os === 'freebsd') {
      if (
        rule.target.os === os && rule.target.arch === arch &&
        rule.target.musl === musl
      ) {
        return name
      }
    }
    if (os === 'win32' || os === 'darwin') {
      if (rule.target.os === os && rule.target.arch === arch) {
        return name
      }
    }
  }
}

function getCommonTargets(
  os = process.platform,
  arch = process.arch,
  musl = isMusl(),
): { target: string; rank: number }[] {
  switch (os) {
    case 'darwin': {
      switch (arch) {
        case 'arm64': {
          return [
            { target: 'darwin-aarch64', rank: 10 },
            { target: 'macos-arm64', rank: 10 },
            { target: 'darwin-arm64', rank: 10 },
            { target: 'mac64arm', rank: 10 },
            { target: 'macos', rank: 1 },
            { target: 'darwin', rank: 1 },
            { target: 'mac', rank: 1 },
            { target: 'universal', rank: 5 },
            { target: 'macos-universal', rank: 10 },
          ]
        }
        case 'x64': {
          return [
            { target: 'macos-amd64', rank: 10 },
            { target: 'darwin-x64', rank: 10 },
            { target: 'darwin-amd64', rank: 10 },
            { target: 'macos-legacy', rank: 10 },
            { target: 'mac64', rank: 10 },
            { target: 'macos', rank: 1 },
            { target: 'darwin', rank: 1 },
            { target: 'mac', rank: 1 },
            { target: 'universal', rank: 5 },
            { target: 'macos-universal', rank: 10 },
          ]
        }
      }
    }
    case 'linux': {
      switch (arch) {
        case 'arm64': {
          if (musl) {
            return [
              { target: 'linux-arm64-musl', rank: 10 },
              { target: 'linux-aarch64', rank: 10 },
              { target: 'linux-riscv64', rank: 10 },
              { target: 'linux', rank: 1 },
            ]
          }
          return [
            { target: 'linux-armv7', rank: 10 },
            { target: 'linux-arm64', rank: 10 },
            { target: 'linux', rank: 1 },
          ]
        }
        case 'x64': {
          if (musl) {
            return [
              { target: 'linux-amd64-musl', rank: 10 },
              { target: 'linux-x64-musl', rank: 10 },
              { target: 'linux-amd64', rank: 10 },
              { target: 'linux-x86_64', rank: 10 },
              { target: 'linux-x64', rank: 5 },
              { target: 'linux-x86', rank: 5 },
              { target: 'linux', rank: 1 },
            ]
          }
          return [
            { target: 'linux-amd64', rank: 10 },
            { target: 'lin64', rank: 10 },
            { target: 'linux-x64', rank: 10 },
            { target: 'linux-x86', rank: 10 },
            { target: 'linux-x86_64', rank: 10 },
            { target: 'linux', rank: 1 },
          ]
        }
      }
    }

    case 'win32': {
      switch (arch) {
        case 'x64': {
          return [
            { target: 'win32-x64', rank: 10 },
            { target: 'win-x64', rank: 10 },
            { target: 'win64', rank: 10 },
            { target: 'windows-amd64', rank: 10 },
            { target: 'windows-x86', rank: 10 },
            { target: 'windows-x64', rank: 10 },
            { target: 'windows-x86_64', rank: 10 },
            { target: 'win', rank: 10 },
            { target: 'x86_64', rank: 5 },
          ]
        }
        case 'arm64': {
          return [
            { target: 'windows-arm64', rank: 10 },
            { target: 'win32-arm64', rank: 10 },
          ]
        }
      }
    }
  }

  return []
}
