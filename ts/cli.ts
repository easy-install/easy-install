import { install } from './install'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'

const [url, name, version] = process.argv.slice(2)

install({
  url,
  version,
  name,
}).then((output) => {
  for (const item of output) {
    console.log(JSON.stringify(item))
    const { installDir } = item
    if (installDir && !hasPath(installDir)) {
      addPath(installDir)
      if (isGithub()) {
        addGithubPath(installDir)
      }
    }
  }
})
