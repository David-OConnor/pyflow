# Changelog

## v0.2.5
- Added support for `manylinux2014` spec
- If a dependencies dependencies are specified multiple times, merge `extras`
by omission. This led to bugs where dependencies didn't get installed when
specified both as an extras and as not
- Dependencies specifying `sys_platform == "win32"` now applies to 64-bit
Windows installations as well. This should fix cases where Windows dependencies
weren't being installed.
- Fixed a dependency-installation bug triggered by symlinks inside Pypi source archives
- `pyflow new` no longer creates a `LICENSE` file

## v0.2.4
- Fixed a bug where `pyflow script` was broken
- Fixed a bug where `pyflow init` was broken
- Fixed parsing `Pipfile`
- `pyflow switch` now sets up the environment/dependencies with the new version
- Fixed a bug where `bsd` specified as an OS on Pypi would cause a crash

## v0.2.3
- Fixed a potential conflict between the built-in `typing` module, and one on pypi
- Now can parse deps with brackets listed in requirements
- Now supports dependencies specified using `~=`. (Treat same as `~`)

## v0.2.1
- Running `pyflow install` is now no longer required; Running `pyflow`, `pyflow list` etc
will now install dependencies as required from `pyproject.toml`.
- `pyflow new` now asks for the Python version instead of using a default.
- Now searches parent directories for `pyproject.toml`, if we can't find one
in the current path.

## v0.1.9
- Can now parse subdependencies of `path` requirements from built-wheels
- Fixed a bug where subdep constraints specified on multiple lines would
cause resolution to fail
- Fixed a bug parsing METADATA requirements that includes extras, but no version

## v0.1.8
- Fixed a bug in auto-filling name and email in `pyflow init` and `pyflow new`
- Running `pyflow` alone in a directory without a `pyproject.toml` will now no
longer attempt to initialize a project
- Added support for specifying a build script
- Treat `python_version` on `pypi` as a caret requirement, if specified like `3.6`.
- Improved error messages

## v0.1.7
- Fixed bugs in `path` dependencies

## v0.1.6
- Added installation from local paths and Git repositories
- Improved error messages and instructions

## v0.1.5
- Combined `author` and `author_email` cfg into one field, `authors`, which takes
- a list. Populates automatically from git. `pyflow new` creates
 a new git repository. (Breaking)
- Fixed a bug with uninstalling packages that use non-standard naming conventions
- Fixed a bug with installing on Mac
- Fixed a bug uninstalling packages from the CLI

## v0.1.4
- Clear now lets the user choose which parts of the cache to clear
- Fixed a bug with dev reqs
- Fixed a bug with CLI-added deps editing `pyproject.toml`
- Added `--dev` flag to `install`

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
- Fixed parsing pypi requirements that omit parentheses
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
