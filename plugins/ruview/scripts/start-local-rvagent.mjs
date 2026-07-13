#!/usr/bin/env node

// Start the in-repository MCP server instead of downloading the historical
// @ruvnet/rvagent package. A clean clone is bootstrapped from the pinned lock
// file; later starts reuse the local build.
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const here = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(here, '..', '..', '..', 'tools', 'ruview-mcp');
const entrypoint = resolve(packageDir, 'dist', 'index.js');
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';

function run(args) {
  const result = spawnSync(npm, args, { cwd: packageDir, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

if (!existsSync(resolve(packageDir, 'node_modules'))) {
  run(['ci', '--ignore-scripts', '--no-audit', '--no-fund']);
}
if (!existsSync(entrypoint)) {
  run(['run', 'build']);
}

await import(pathToFileURL(entrypoint).href);
