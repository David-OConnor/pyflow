# Changelog

## v0.0.4
- Added support for running minimally-configured scripts
- Implemented `pyflow switch` to change py versions. Improved related prompts
- Misc API tweaks

## v0.0.3
- Now manages and installs Python as required
- Stores downloaded packages in a global cache
- Can run console scripts specified in `pyproject.toml` directly, instead of just
ones installed by dependencies
- `pyflow reset` now cleans up the lock file
- Misc tweaks and bugfixes