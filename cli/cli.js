#!/usr/bin/env node

const p = require('path').join(__dirname, '..', 'cjs', 'cli.js')
if (require('fs').existsSync(p)) {
  require(p)
}
