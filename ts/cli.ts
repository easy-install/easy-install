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
  for (const item of output) {
    const { installDir } = item
    if (installDir && !hasPath(installDir)) {
      addPath(installDir)
      if (isGithub()) {
        addGithubPath(installDir)
      }
    }
  }

  if (output.length === 1) {
    const first = output[0].installPath
    if (first) {
      addExecutePermission(first)
    }
  }
})
