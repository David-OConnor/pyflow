# Changelog

## v0.0.3
- Now manages and installs Python as required
- Stores downloaded packages in a global cache
- Can run console scripts specified in `pyproject.toml` directly, instead of just
ones installed by dependencies
- `pypackage reset` now cleans up the lock file
- Misc tweaks and bugfixes