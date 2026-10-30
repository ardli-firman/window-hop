#!/usr/bin/env python3
"""
Version management helper for Window Hop.

Usage:
    python3 scripts/version.py get
    python3 scripts/version.py full
    python3 scripts/version.py set <VERSION>
    python3 scripts/version.py bump <patch|minor|major>
"""

import os
import re
import subprocess
import sys

CARGO_TOML_PATH = os.path.join(os.path.dirname(__file__), "..", "Cargo.toml")
SEMVER_REGEX = re.compile(
    r"^(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<prerelease>(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+(?P<buildmetadata>[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)


def get_current_version(path=CARGO_TOML_PATH):
    with open(path, "r", encoding="utf-8") as f:
        in_package = False
        for line in f:
            stripped = line.strip()
            if stripped == "[package]":
                in_package = True
                continue
            if in_package and stripped.startswith("["):
                break
            if in_package:
                m = re.match(r'^version\s*=\s*"([^"]+)"', stripped)
                if m:
                    return m.group(1)
    raise ValueError(f"Could not find [package] version in {path}")


def get_git_describe():
    try:
        out = subprocess.check_output(
            ["git", "describe", "--tags", "--always", "--dirty"],
            stderr=subprocess.DEVNULL,
            encoding="utf-8",
        ).strip()
        return out
    except Exception:
        return ""


def set_version(new_version, path=CARGO_TOML_PATH):
    if not SEMVER_REGEX.match(new_version):
        raise ValueError(f"Invalid semver version: {new_version}")

    with open(path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    in_package = False
    replaced = False
    new_lines = []

    for line in lines:
        stripped = line.strip()
        if stripped == "[package]":
            in_package = True
            new_lines.append(line)
            continue
        if in_package and stripped.startswith("["):
            in_package = False

        if in_package and not replaced and re.match(r'^version\s*=\s*"[^"]+"', stripped):
            new_lines.append(f'version = "{new_version}"\n')
            replaced = True
        else:
            new_lines.append(line)

    if not replaced:
        raise ValueError(f"Could not locate version line in [package] section of {path}")

    with open(path, "w", encoding="utf-8") as f:
        f.writelines(new_lines)

    # Update Cargo.lock if cargo is available
    try:
        subprocess.run(
            ["cargo", "check", "--quiet"],
            cwd=os.path.dirname(os.path.abspath(path)),
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        pass


def bump_version(part, path=CARGO_TOML_PATH):
    current = get_current_version(path)
    m = SEMVER_REGEX.match(current)
    if not m:
        raise ValueError(f"Current version '{current}' is not valid semver")

    major = int(m.group("major"))
    minor = int(m.group("minor"))
    patch = int(m.group("patch"))

    part = part.lower()
    if part == "patch":
        patch += 1
    elif part == "minor":
        minor += 1
        patch = 0
    elif part == "major":
        major += 1
        minor = 0
        patch = 0
    else:
        raise ValueError(f"Unknown bump target '{part}'. Choose from: patch, minor, major")

    new_version = f"{major}.{minor}.{patch}"
    set_version(new_version, path)
    return new_version


def main():
    if len(sys.argv) < 2:
        print(__doc__.strip())
        sys.exit(1)

    cmd = sys.argv[1].lower()

    if cmd == "get":
        print(get_current_version())
    elif cmd == "full":
        ver = get_current_version()
        git_info = get_git_describe()
        if git_info:
            print(f"{ver} ({git_info})")
        else:
            print(ver)
    elif cmd == "set":
        if len(sys.argv) < 3:
            eprint("Error: Missing version argument. Example: python3 scripts/version.py set 0.2.0")
            sys.exit(1)
        new_ver = sys.argv[2]
        set_version(new_ver)
        print(f"Updated version to {new_ver}")
    elif cmd == "bump":
        if len(sys.argv) < 3:
            eprint("Error: Missing bump part. Example: python3 scripts/version.py bump patch")
            sys.exit(1)
        part = sys.argv[2]
        new_ver = bump_version(part)
        print(f"Bumped {part} version to {new_ver}")
    else:
        print(f"Unknown command: {cmd}")
        print(__doc__.strip())
        sys.exit(1)


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


if __name__ == "__main__":
    main()
