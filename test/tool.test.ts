import { expect, test } from "vitest"
import { download, extractTo, getAssetNames, isArchiveFile } from "../ts/tool"
import * as path from "path"
import * as fs from "fs"
import { homedir, tmpdir } from "os"
import { downloadToFile } from "../ts/download"
import { join } from "path"
import { Repo } from "../ts"
import { getArtifact, hasFile } from "../ts/dist-manifest"
import { assert } from "console"

test("getAssetNames", () => {
  expect(getAssetNames("deno", "win32", "x64")).toEqual([
    "deno-x86_64-pc-windows-msvc",
    "deno-x86_64-pc-windows-gnu",
  ])
  expect(getAssetNames("deno", "linux", "x64")).toEqual([
    "deno-x86_64-unknown-linux-gnu",
    "deno-x86_64-unknown-linux-musl",
  ])
  expect(getAssetNames("deno", "darwin", "x64")).toEqual([
    "deno-x86_64-apple-darwin",
  ])
  expect(getAssetNames("deno", "darwin", "arm64")).toEqual([
    "deno-aarch64-apple-darwin",
  ])
})

test("extractTo zip", async () => {
  const url =
    "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"
  const filePath = path.join(tmpdir(), "ansi2-x86_64-pc-windows-msvc.zip")
  const testDir = "easy-setup-test"
  const installDir = path.join(homedir(), testDir)
  await download(url, filePath)
  extractTo(filePath, installDir)
  const ansi2Path = path.join(homedir(), testDir, "ansi2.exe")
  expect(fs.existsSync(ansi2Path)).toEqual(true)
}, 100_000)

test("extractTo tar.gz", async () => {
  // only test on linux
  if (process.platform === "win32") return
  const url =
    "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-aarch64-apple-darwin.tar.gz"
  const filePath = path.join(tmpdir(), "ansi2-aarch64-apple-darwin.tar.gz")
  const testDir = "easy-setup-test"
  const installDir = path.join(homedir(), testDir)
  await download(url, filePath)
  extractTo(filePath, installDir)
  const ansi2Path = path.join(homedir(), testDir, "ansi2")
  expect(fs.existsSync(ansi2Path)).toEqual(true)
}, 100_000)

test("isArchiveFile", () => {
  for (
    const [url, ty] of [
      ["https://github.com/ahaoboy/ansi2", false],
      ["https://api.github.com/repos/ahaoboy/ansi2/releases/latest", false],
      ["https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11", false],
      [
        "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz",
        true,
      ],
      [
        "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip",
        true,
      ],
    ] as const
  ) {
    expect(isArchiveFile(url)).toEqual(ty)
  }
})

test("extractTo", async () => {
  const url = "https://github.com/ahaoboy/mujs-build/archive/refs/tags/v0.0.4.zip"
  const tmpPath = await downloadToFile(url)
  const tmpDir = await extractTo(tmpPath)
  expect(fs.existsSync(join(tmpDir, 'mujs-build-0.0.4', 'dist-manifest.json'),)).toEqual(true)
})


test("manifest_jsc", async () => {
  const repo = new Repo('ahaoboy', 'jsc-build')
  const dist = await repo.getManfiest()
  const art = getArtifact(dist, ["x86_64-unknown-linux-gnu"])!
  for (const [k, v] of [
    ['bin/jsc', true],
    ['lib/libJavaScriptCore.a', true],
    ['lib/jsc', false],
  ] as const) {
    expect(hasFile(art, k)).toEqual(v)
  }
})

test("manifest_mujs", async () => {
  const repo = new Repo('ahaoboy', 'jsc-build')
  const dist = await repo.getManfiest()
  const art = getArtifact(dist, ["x86_64-unknown-linux-gnu"])!
  for (const [k, v] of [
    ['mujs', true],
    ['mujs.exe', false],
  ] as const) {
    expect(hasFile(art, k)).toEqual(v)
  }
  const artWin = getArtifact(dist, ["x86_64-pc-windows-gnu"])!
  for (const [k, v] of [
    ['mujs', false],
    ['mujs.exe', true],
  ] as const) {
    expect(hasFile(artWin, k)).toEqual(v)
  }
})
