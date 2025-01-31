import { run } from './run'

const [url, name, version] = process.argv.slice(2)

run({
  url,
  version,
  name,
})
