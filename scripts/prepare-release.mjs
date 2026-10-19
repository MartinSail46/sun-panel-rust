import assert from 'node:assert/strict'
import { appendFileSync, readFileSync } from 'node:fs'
import process from 'node:process'

const version = JSON.parse(readFileSync('package.json', 'utf8')).version
assert.match(version, /^\d+\.\d+\.\d+$/, 'release version must be stable semver')
assert.equal(process.env.GITHUB_REF_NAME, `v${version}`, 'tag must match package.json')
const cargo = readFileSync('service-rust/Cargo.toml', 'utf8')
assert.equal(cargo.match(/^version = "([^"]+)"/m)?.[1], version, 'Cargo version must match package.json')
const notes = `doc/releases/${version}.md`
assert.ok(readFileSync(notes, 'utf8').trim(), 'release notes must not be empty')
const output = `version=${version}\nnotes=${notes}\n`
if (process.env.GITHUB_OUTPUT)
  appendFileSync(process.env.GITHUB_OUTPUT, output)
else
  process.stdout.write(output)
