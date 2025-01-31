import { expect, test } from "vitest"
import { downloadDistManfiest } from "../ts/download"

test("downloadDistManfiest", async () => {
  const json = await downloadDistManfiest("https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json")
  expect(json.artifacts['cargo-dist-x86_64-pc-windows-msvc.zip'].kind).toEqual('executable-zip')
})