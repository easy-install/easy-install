import { extensions, Fmt } from '@easy-install/easy-archive'
export const DefaultMode = 0o755
export const WINDOWS_EXE_EXTS = [
  '.exe',
  '.ps1',
  '.bat',
  '.cmd',
  '.com',
  '.vbs',
]

export const INSTALLER_EXTS = [
  '.msi',
  '.msix',
  '.appx',
  '.deb',
  '.rpm',
  '.dmg',
  '.pkg',
  '.app',
  '.apk',
  '.ipa',
]

export const TEXT_FILE_EXTS = [
  '.txt',
  '.md',
  '.json',
  '.xml',
  '.csv',
  '.log',
  '.ini',
  '.cfg',
  '.conf',
  '.yaml',
  '.yml',
]

export const SKIP_FMT_LIST = [
  '.sha256sum',
  '.sha256',
  '.sha1',
  '.md5',
  '.sum',
  '.msi',
  '.msix',
  '.appx',
  '.app',
  '.appimage',
  '.json',
  '.txt',
  '.md',
  '.log',
  '.sig',
  '.asc',
]

export const ArchiveFmtList = [
  Fmt.Tar,
  Fmt.TarBz,
  Fmt.TarGz,
  Fmt.TarXz,
  Fmt.TarZstd,
  Fmt.Zip,
].map((i) => extensions(i)).flat()

export const MAX_FILE_COUNT = 16

export const ALL_FILE_EXTS = [
  ...new Set([
    ...ArchiveFmtList,
    ...WINDOWS_EXE_EXTS,
    ...INSTALLER_EXTS,
    ...TEXT_FILE_EXTS,
    ...SKIP_FMT_LIST,
  ]),
]
