[![crates.io version](https://meritbadge.herokuapp.com/pypackage)](https://crates.io/crates/pypackage)
[![docs.rs](https://docs.rs/pypackage/badge.svg)](https://docs.rs/pypackage)
[![Build Status](https://travis-ci.org/David-OConnor/pypackage.svg?branch=master)](https://travis-ci.org/David-OConnor/pypackage)


# Py Packages

This tool implements
[PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/). 
It manages dependencies, keeping them isolated in the project directory, and runs
python in an environment which uses this directory. Per PEP 582, dependencies
are stored in the project directory → `__pypackages__` → `3.7`(etc) → `lib`.

**Goal**: Make using and publishing Python projects as simple as possible. Understanding
Python environments shoudn't be required to use dependencies safely.

Python ≥ 3.4 is required.


## Installation
There are 2 ways to install:
- Download a binary from the [releases](https://github.com/David-OConnor/pypackage/releases)
 page. Installers are available for Debian/Ubuntu, and Windows. On Debian or Ubuntu, download and run
[this deb](https://github.com/David-OConnor/pypackage/releases/download/0.0.2/pypackage_0.0.2_amd64.deb). 
On Windows, download and run
[this installer](https://github.com/David-OConnor/pypackage/releases/download/0.0.2/pypackage-0.0.2-x86_64.msi). 
Alternatively, download the appropriate binary (ie `pypackage.exe` or `pypackage`) and place it somewhere
accessible by the system path. For example, `/usr/bin` in linux, 
or `~\AppData\Local\Programs\Python\Python37\bin` in Windows.

- If you have [Rust](https://www.rust-lang.org) installed, the most convenient way is to 
run `cargo install pypackage`.

## Quickstart
- *(Optional)* Run `pypackage init` in an existing project folder, or `pypackage new projname` 
to create a new project folder. `init` imports data from `requirements.txt` or `Pipfile`; `new`
creates a folder with the basics
- Run `pypackage install` to sync dependencies with `pyproject.toml`, or add dependencies to it
- Run `pypackage python` to run Python


## Why add another Python dependency manager?
`Pipenv` and `Poetry` both address this problem. Goal: Faster and less finicky.
 Some reasons why this tool is different:

- Its dependency resolution and locking is faster due to using a cached
database of dependencies, vice downloading and checking each package, or relying
on the incomplete data available on the [pypi warehouse](https://github.com/pypa/warehouse).

- By not requiring Python to install or run, it remains intallation-agnostic and 
environment-agnostic. This is important for making setup and use as simple and decison-free as
 possible. It's especially important on Linux, where there may be several versions
of Python installed, with different versions and access levels. This avoids
complications, especially for new users. It's common for Python-based CLI tools
to not run properly when installed from `pip` due to the `PATH` 
not being configured in the expected way.

- It keeps dependencies in the project directory, in `__pypackages__`, and
doesn't modify outside files.

- If multiple Python installations are found, it allows the user to select the desired 
one to use for each project. This is a notable problem with `Poetry`; it
may pick the wrong installation (eg Python2 vice Python3), with no obvious way to change it.

- Multiple versions of a dependency can be installed, allowing resolution
of conflicting sub-dependencies.


## Virtual environments are easy. What's the point of this?
Hopefully we're not replacing [one problem](https://xkcd.com/1987/) with [another](https://xkcd.com/927/).

Some people like the virtual-environment workflow - it requires only tools included 
with Python, and uses few console commands to create,
and activate and environments. However, it may be tedius depending on workflow:
The commands may be long depending on the path of virtual envs and projects,
and it requires modifying the state of the terminal for each project, each time
you use it, which you may find inconvenient or inelegant.

If you're satisified with an existing flow, there may be no reason to change, but
I think we can do better. This is especially relevant for new Python users
who haven't groked venvs, or are unaware of the hazards of working with a system Python.
 
`Pipenv` improves the workflow by automating environment use, and 
allowing reproducable dependency resolution. `Poetry` improves upon `Pipenv's` API,
speed, and dependency resolution, as well as improving
the packaging and distributing process by using a consolidating project config. Both
 are sensitive to the Python environment used to run them. This tool
attempts to improve upon both in the areas listed in the section above. Its goal is to be
as intuitive as possible.

`Conda` addresses these problems elegantly, but maintains a separate repository
of binaries from `PyPi`. If all packages you need are available on `Conda`, it may
be the best solution. If not, it requires falling back to `Pip`, which means 
using two separate package managers.

When building and deploying packages, a set of degenerate files are 
traditionally used: `setup.py`, `setup.cfg`, `requirements.txt` and `MANIFEST.in`. We use
`pyproject.toml` as the single-source of project info required to build
and publish.


## A thoroughly biased feature table
(Please PR anything here that's innacurate, incomplete, or misleading)

These tools have different scopes and purposes:

| Name | [Pip + venv](https://docs.python.org/3/library/venv.html) | [Pipenv](https://docs.pipenv.org) | [Poetry](https://poetry.eustace.io) | [pyenv](https://github.com/pyenv/pyenv) | [pythonloc](https://github.com/cs01/pythonloc) | [Conda](https://docs.conda.io/en/latest/) |this |
|------|------------|--------|--------|-------|-----------|-------|-----|
| **Manages dependencies** | ✓ | ✓ | ✓ | | | ✓ | ✓|
| **Py-environment-agnostic** | | | | ✓ | | ✓ | ✓ |
| **Included with Python** | ✓ | | | | | | |
| **Stores packages with project** | | | | | ✓ | | ✓|
| **Locks dependencies** |  | ✓ | ✓ | | | ✓ | ✓|
| **Requires changing session state** | ✓ | | | ✓ | | | |
| **Slow** |  | ✓ | | | | | |
| **Clean build/publish flow** | | | ✓ | | | | ✓ |
| **Buggy** | | | | | | | ✓ |
| **Supports old Python versions** | with `virtualenv` | ✓ | ✓ | ✓ | ✓ | ✓ | |


## Use
- Create a `pyproject.toml` file in your project directory. Note that running
 `init`, `new`, or `install` creates this file automatically. See
[PEP 518](https://www.python.org/dev/peps/pep-0518/) for details.

Example contents:
```toml
[tool.pypackage]
py_version = "3.7"
name = "runcible"
version = "0.1.0"
author = "John Hackworth"


[tool.pypackage.dependencies]
numpy = "^1.16.4"
diffeqpy = "1.1.0"
```
The `[tool.pypackage]` section is used for metadata, and isn't required unless
building and distributing a package. The `[tool.pypyackage.dependencies]` section
contains all dependencies, and is an analog to `requirements.txt`. 

You can specify `extra` dependencies, which will only be installed when passing
explicit flags to `pypackage install`, or when included in another project with the appropriate
 flag enabled. Ie packages requirng this one can enable with 
`pip install -e` etc.
```toml
[tool.pypackage.extras]
test = ["pytest", "nose"]
secure = ["crypto"]
```

If you'd like to an install a dependency with extras, use syntax like this:
```toml
[tool.pypackage.dependencies]
ipython = { version = "^7.7.0", extras = ["qtconsole"] }
```

For details on 
how to specify dependencies in this `Cargo.toml`-inspired 
[semvar](https://semver.org) format,
 reference
[this guide](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html).

We also attempt to parse metadata and dependencies from [tool.poetry](https://poetry.eustace.io/docs/pyproject/)
sections of `pyproject.toml`, so there's no need to modify the format
if you're using that.


## What you can do

### Managing dependencies:
- `pypackage install` - Install all packages in `pyproject.toml`, and remove ones not (recursively) specified
- `pypackage install toolz` - If you specify one or more packages after `install`, those packages will 
be added to `pyproject.toml` and installed.
- `pypackage install numpy==1.16.4 matplotlib>=3.1.` - Example with multiple dependencies, and specified versions
- `pypackage uninstall toolz` - Remove one or more dependencies

### Running REPL and Python files in the environment:
- `pypackage python` - Run a Python REPL
- `pypackage python main.py` - Run a python file
- `pypackage ipython`, `pypackage black` etc - Run a CLI script like `ipython`. This can either
have been installed by a dependency, or specified under `[tool.pypackage]`, `scripts`
- `pypackage run ipython` - alternate syntax

### Building and publishing:
- `pypackage package` - Package for distribution (uses setuptools internally, and 
builds both source and wheel if applicable.)
- `pypackage package --features "test all"` - Package for distribution with features enabled, 
as defined in `pyproject.toml`
- `pypackage publish` - Upload to PyPi (Repo specified in `pyproject.toml`. Uses `Twine` internally.)

### Misc:
- `pypackage list` - Display all installed packages and console scripts
- `pypackage new projname` - Create a directory containing the basics for a project: 
a readme, pyproject.toml, .gitignore, and directory for code
- `pypackage init` - Create a `pyproject.toml` file in an existing project directory. Pull info from
`requirements.text` and `Pipfile` as required.
- `pypackage reset` - Remove the environment, and uninstall all packages
- `pypackage -V` - Get the current version of this tool
- `pypackage help` Get help, including a list of available commands


## How installation and locking work
Running `pypackage install` syncs the project's installed dependencies with those
 specified in `pyproject.toml`. It generates `pypackage.lock`, which on subsequent runs,
  keeps dependencies each package a fixed version, as long as it continues to meet the constraints
  specified in `pyproject.toml`. Adding a
package name via the CLI, eg `pypackage install matplotlib` simply adds that requirement before proceeding.
`pypackage.lock` isn't meant to be edited directly.
 
Each dependency listed in `pyproject.toml` is checked for a compatible match in `pypackage.lock`
 If a constraint is met by something in the lock file, 
the version we'll sync will match that listed in the lock file. If not met, a new entry
is added to the lock file, containing the highest version allowed by `pyproject.toml`.
Once complete, packages are installed and removed in order to exactly meet those listed
in the updated lock file.

This tool downloads and unpacks wheels from `pypi`, or builds
wheels from source if none are availabile. It verifies the integrity of the downloaded file
 against that listed on `pypi` using `SHA256`, and the exact 
versions used are stored in a lock file.

When a dependency is removed from `pyproject.toml`, it, and its subdependencies not
also required by other packages are removed from the `__pypackages__` folder.


## How dependencies are resolved

Compatible versions of dependencies are determined using info from 
the [PyPi Warehouse](https://github.com/pypa/warehouse) (available versions, and hash info), 
and the `pydeps` database. We use `pydeps`, which is built specifically for this project,
due to inconsistent dependency information stored on `pypi`. A dependency graph is built
using this cached database. We attempt to use the newest compatible version of each package.

If all packages are either only specified once, or specified multiple times with the same
newest-compatible version, we're done resolving, and ready to install and sync.

If a package is included more than once with different newest-compatible versions, but one
of those newest-compatible is compatible with all requirements, we install that one. If not,
we search all versions to find one that's compatible.

If still unable to find a version of a package that satisfies all requirements, we install
multiple versions of it as-required, store them in separate directories, and modify
their parents' imports as required.

Note that it may be possible to resolve dependencies in cases not listed above, instead
of installing multiple versions. Ie we could try different combinations of top-level packages,
check for resolutions, then vary children as-required down the hierarchy. We don't do this because
 it's slow, has no guarantee of success, and involves installing older versions of packages.


## Not-yet-implemented

- Installing from sources other than `pypi` (eg repos)
- Installing multiple versions of a dependency may not work if it uses compiles code
- The lock file is missing some info like hashes
- Adding a dependency via the CLI with a specific version constraint, or extras.
- Developer requirements
- Packaging and publishing projects that use compiled extensions
- Global package cache to avoid resolving and downloading the same package 
for each project?


## Building and uploading your project to PyPi.
In order to build and publish your project, additional info is needed in
`pyproject.toml`, that mimics what would be in `setup.py`. Example:
```toml
[tool.pypackage]
name = "everythingkiller"
py_version = "3.6"
version = "0.1.0"
author = "Fraa Erasmas"
author_email = "raz@edhar.math"
description = "Small, but packs a punch!"
homepage = "https://everything.math"
repository = "https://github.com/raz/everythingkiller"
license = "MIT"
classifiers = [
    "Topic :: System :: Hardware",
    "Topic :: Scientific/Engineering :: Human Machine Interfaces",
]
scripts = { activate = "jeejah:activate" }


[tool.pypackage.dependencies]
numpy = "^1.16.4"
manim = "0.1.8"
ipython = {version = "^7.7.0", extras=["qtconsole"]}
```

## Building this from source                      
If you’d like to build from source, [download and install Rust]( https://www.rust-lang.org/tools/install),
clone the repo, and in the repo directory, run `cargo build --release`.

Ie on Linux:
```bash
curl https://sh.rustup.rs -sSf | sh
git clone https://github.com/david-oconnor/pypackage.git
cd pypackage
cargo build --release
```

## Updating
If installed via `Cargo`, run `cargo install pypackage --force`.

## Contributing
If you notice unexpected behavior or missing features, please post an issue,
or submit a PR. If you see unexpected
behavior, it's probably a bug! Post an issue listing the dependencies that did
not install correctly.


## Dependency cache repo:
- [Github](https://github.com/David-OConnor/pydeps)
Example API call: `https://pydeps.herokuapp.com/numpy`. This pulls all top-level
dependencies for the `numpy` package. The first time this command is run
for a package/version combo, it may be slow. Subsequent calls, by anyone,
should be fast. This is due to having to download and install each package
on the server to properly determine dependencies, due to unreliable information
 on the `pypi warehouse`.


## Gotchas
- Make sure the `pypackage` binary is accessible in your path. If installing
via a `deb`, `msi`, or `Cargo`, this should be set up automatically.
- Make sure `__pypackages__` and `.venv` are in your `.gitignore` file.

# References
- [PEP 582 - Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
- [Pep 518 - pyproject.toml](https://www.python.org/dev/peps/pep-518/)
- [Semantic versioning](https://semver.org/)
- [Specifying dependencies in Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [Predictable dependency management blog entry](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html)
- [Blog on why Pyhon dependencies are hard to determine](https://dustingram.com/articles/2018/03/05/why-pypi-doesnt-know-dependencies/)