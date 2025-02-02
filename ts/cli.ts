import { install } from './install'
import { addGithubPath, addPath, hasPath, isGithub } from 'crud-path'

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
})
