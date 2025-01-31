#!/usr/bin/env node

const p = require('path').join(__dirname, '..', 'bundle', 'cli.js')
if (require('fs').existsSync(p)) {
  require(p)
}
