import { install } from './install'

const [url, name, version] = process.argv.slice(2)

install({
  url,
  version,
  name,
})
