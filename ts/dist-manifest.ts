import { matchTargets, removePostfix } from "./tool";
import { Artifact, DistManifest } from "./type";

export function getArtifact(dist: DistManifest, targets: string[]): Artifact | undefined {
  for (const art of Object.values(dist.artifacts)) {
    if (
      matchTargets(art.target_triples, targets)
      && (art.kind || "executable-zip") === "executable-zip"
    ) {
      return art
    }
  }
}


export function hasFile(art: Artifact, path: string) {
  path = path.replaceAll('\\', '/')
  if (art.name) {
    const prefix = removePostfix(art.name) + "/"
    if (path.startsWith(prefix)) {
      path = path.slice(prefix.length)
    }
  }


  for (const i of art.assets ?? []) {
    if (path === "*") {
      // FIXME: support regex
      return true
    }

    if (path === i.path) {
      switch (i.kind) {
        case "executable":
        case "c_dynamic_library":
        case "c_static_library": {
          return true
        }
        // case "readme":
        // case "license":
        // case "changelog":
        default: {
          return false
        }
      }
    }
  }

  return false
}