import { Repo } from "../repo"
import { download, extractTo } from "../tool"
import type { Input, Output } from "../type"

export async function install(
  input: Input,
  installDir: string,
): Promise<Output> {
  const { url, version = "latest", bin } = input
  const downloadUrl = await  Repo.fromUrl(url)!.getAssetUrl(
    bin?.length ? bin : undefined,
    version,
  )
  const downloadPath = await download(downloadUrl)
  await extractTo(downloadPath, installDir)
  return {
    version,
    installDir,
    downloadUrl,
  }
}
