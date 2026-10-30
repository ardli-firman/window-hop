#!/usr/bin/env node
/**
 * Version management helper for Window Hop (Node.js).
 * Requires only native Node.js (no npm packages required).
 *
 * Usage:
 *   node scripts/version.js get
 *   node scripts/version.js full
 *   node scripts/version.js set <VERSION>
 *   node scripts/version.js bump <patch|minor|major>
 */

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const CARGO_TOML_PATH = path.join(__dirname, '..', 'Cargo.toml');
const SEMVER_REGEX = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;

function getCurrentVersion(filePath = CARGO_TOML_PATH) {
  const content = fs.readFileSync(filePath, 'utf8');
  let inPackage = false;
  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === '[package]') {
      inPackage = true;
      continue;
    }
    if (inPackage && trimmed.startsWith('[')) {
      break;
    }
    if (inPackage && trimmed.startsWith('version')) {
      const match = trimmed.match(/^version\s*=\s*"([^"]+)"/);
      if (match) {
        return match[1];
      }
    }
  }
  throw new Error(`Could not find [package] version in ${filePath}`);
}

function getGitDescribe() {
  try {
    return execSync('git describe --tags --always --dirty', { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    return '';
  }
}

function setVersion(newVersion, filePath = CARGO_TOML_PATH) {
  if (!SEMVER_REGEX.test(newVersion)) {
    throw new Error(`Invalid semver version: ${newVersion}`);
  }

  const content = fs.readFileSync(filePath, 'utf8');
  const lines = content.split(/\r?\n/);
  let inPackage = false;
  let replaced = false;
  const newLines = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === '[package]') {
      inPackage = true;
      newLines.push(line);
      continue;
    }
    if (inPackage && trimmed.startsWith('[')) {
      inPackage = false;
    }

    if (inPackage && !replaced && /^version\s*=\s*"[^"]+"/.test(trimmed)) {
      newLines.push(`version = "${newVersion}"`);
      replaced = true;
    } else {
      newLines.push(line);
    }
  }

  if (!replaced) {
    throw new Error(`Could not locate version line in [package] section of ${filePath}`);
  }

  fs.writeFileSync(filePath, newLines.join('\n').trimEnd() + '\n', 'utf8');

  // Update Cargo.lock if cargo is available
  try {
    execSync('cargo check --quiet', {
      cwd: path.dirname(path.resolve(filePath)),
      stdio: 'ignore'
    });
  } catch {
    // Ignore if cargo is not in path or in environments without cargo
  }
}

function bumpVersion(part, filePath = CARGO_TOML_PATH) {
  const current = getCurrentVersion(filePath);
  const match = current.match(SEMVER_REGEX);
  if (!match) {
    throw new Error(`Current version '${current}' is not valid semver`);
  }

  let major = parseInt(match[1], 10);
  let minor = parseInt(match[2], 10);
  let patch = parseInt(match[3], 10);

  const target = part.toLowerCase();
  if (target === 'patch') {
    patch += 1;
  } else if (target === 'minor') {
    minor += 1;
    patch = 0;
  } else if (target === 'major') {
    major += 1;
    minor = 0;
    patch = 0;
  } else {
    throw new Error(`Unknown bump target '${part}'. Choose from: patch, minor, major`);
  }

  const newVersion = `${major}.${minor}.${patch}`;
  setVersion(newVersion, filePath);
  return newVersion;
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    console.log('Usage: node scripts/version.js <get|full|set <ver>|bump <patch|minor|major>>');
    process.exit(1);
  }

  const cmd = args[0].toLowerCase();

  switch (cmd) {
    case 'get':
      console.log(getCurrentVersion());
      break;
    case 'full': {
      const ver = getCurrentVersion();
      const gitInfo = getGitDescribe();
      console.log(gitInfo ? `${ver} (${gitInfo})` : ver);
      break;
    }
    case 'set': {
      if (args.length < 2) {
        console.error('Error: Missing version argument. Example: node scripts/version.js set 0.2.0');
        process.exit(1);
      }
      setVersion(args[1]);
      console.log(`Updated version to ${args[1]}`);
      break;
    }
    case 'bump': {
      if (args.length < 2) {
        console.error('Error: Missing bump part. Example: node scripts/version.js bump patch');
        process.exit(1);
      }
      const newVer = bumpVersion(args[1]);
      console.log(`Bumped ${args[1]} version to ${newVer}`);
      break;
    }
    default:
      console.error(`Unknown command: ${cmd}`);
      process.exit(1);
  }
}

main();
