# Changelog

## v0.1.3
- Added support for dev dependencies
- Fixed a bug where dependencies weren't being set up with the `package` command

## v0.1.2
- Added support for installing Python on most Linux distros
- Wheel is now installed directly, instead of with Pip; should only be dependent on
pip now to install `twine`.
- Now doesn't ask to choose between aliases pointing to the same Python install.
- Fixed a bug related to creating `pyflow` directory
- Fixed a bug in specifying package url with the `publish` command.


## v0.1.1
- Fixed a bug, where spaces could prevent console scripts from being installed
- Fixed parsing pypi requirements that ommit parenthesis
- Now uses `~/.local/share/pyflow` on Linux, `~\AppData\Roaming\pyflow` on Windows, and 
`~/Library/Application Support/pyflow` on Mac, instead of `~/.python-installs`

## v0.1.0
- Installing Python binaries now works correctly on Windows, Ubuntu≥18.4, and Debian
- Running `pyflow` with no arguments now runs a Python REPL
- Made error messages more detailed

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