import { install } from './install'
import { addOutputToPath } from './tool'

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
  addOutputToPath(output)
  if (Object.keys(output).length === 0) {
    console.log(`No file installed from ${url}`)
  }
})
