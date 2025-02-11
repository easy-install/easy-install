import { install } from './install'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'
import { addExecutePermission } from './tool'

const [url, name, version] = process.argv.slice(2)

if (!url) {
  console.log('usage:\nei <url>')
  process.exit()
}

install({
  url,
  version,
  name,
}).then((output) => {
  for (const files of Object.values(output)) {
    for (const item of files) {
      const { installDir } = item
      if (installDir && !hasPath(installDir)) {
        addPath(installDir)
        if (isGithub()) {
          addGithubPath(installDir)
        }
      }
    }
    if (files.length === 1 && files[0].installPath) {
      const first = files[0].installPath
      if (first) {
        addExecutePermission(first)
      }
    }
  }
})
