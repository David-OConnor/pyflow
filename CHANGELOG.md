# Changelog

## v0.1.0
- Installing Python binaries now works correctly on Windows, Ubuntu≥18.4, and Debian
- Running `pyflow` with no arguments now runs a Python REPL

## v0.0.4
- Renamed from `pypackage` to `pyflow`
- Added support for running minimally-configured scripts
- Implemented `pyflow switch` to change py versions. Improved related prompts
- Misc API tweaks

## v0.0.3
- Now manages and installs Python as required
- Stores downloaded packages in a global cache
- Can run console scripts specified in `pyproject.toml` directly, instead of just
ones installed by dependencies
- `pypackage reset` now cleans up the lock file
- Misc tweaks and bugfixes