import { install } from './install'
import { addGithubPath, addPath, hasPath } from 'crud-path'

const [url, name, version] = process.argv.slice(2)

install({
  url,
  version,
  name,
}).then(output => {
  for (const { installDir } of output) {
    if (installDir && !hasPath(installDir)) {
      addPath(installDir)
      addGithubPath(installDir)
    }
  }
})
