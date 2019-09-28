[![crates.io version](https://meritbadge.herokuapp.com/pyflow)](https://crates.io/crates/pyflow)
[![Build Status](https://travis-ci.org/David-OConnor/pyflow.svg?branch=master)](https://travis-ci.org/David-OConnor/pyflow)


# Pyflow

#### *Simple is better than complex* - The Zen of Python

This tool manages Python installations and dependencies.

![Poetry Install](https://raw.githubusercontent.com/david-oconnor/pyflow/master/assets/install.gif)

**Goals**: Make using and publishing Python projects as simple as possible. Understanding
Python environments shouldn't be required to use dependencies safely. We're attempting
to fix each stumbling block in the Python workflow, so that it's as elegant
as the language itself.

You don't need Python or any other tools installed to use Pyflow.

It can run standalone scripts in their
own environments with no config, and functions directly from the CLI.

It implements [PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
and [Pep 518 (pyproject.toml)](https://www.python.org/dev/peps/pep-0518/), and supports Python ≥ 3.4.  


## Installation
- **Windows, Ubuntu, or Debian:** Download and run
[this installer](https://github.com/David-OConnor/pyflow/releases/download/0.1.3/pyflow-0.1.3-x86_64.msi)
or
[this deb](https://github.com/David-OConnor/pyflow/releases/download/0.1.3/pyflow_0.1.3_amd64.deb) .

- **A different Linux distro:** Download this [standalone binary](https://github.com/David-OConnor/pyflow/releases/download/0.1.3/pyflow)
 and place it somewhere
accessible by the system PATH. For example, `/usr/bin`.

- **If you have [Rust](https://www.rust-lang.org) installed**: Run `cargo install pyflow`.

- **Mac:**  Build from source using the instructions near the bottom of this page,
 or install via `cargo`. If able, please PR the binary.

## Quickstart
- *(Optional)* Run `pyflow init` in an existing project folder, or `pyflow new projname` 
to create a new project folder. `init` imports data from `requirements.txt` or `Pipfile`; `new`
creates a folder with the basics.
- Run `pyflow install` in a project folder to sync dependencies with `pyproject.toml`, 
or add dependencies to it. 
this file will be created if it doesn't exist.
- Run `pyflow` or `pyflow myfile.py` to run Python.


## Quick-and-dirty start for quick-and-dirty scripts
- Add the line `__requires__ = [numpy, requests]` somewhere in your script, where `numpy` and 
`requsts` are dependencies.
Run `pyflow script myscript.py`, where `myscript.py` is the name of your script.
This will set up an isolated environment for this script, and install
dependencies as required. This is a safe way
to run one-off Python files that aren't attached to a project, but have dependencies.


## Why add another Python manager?
`Pipenv`, `Poetry`, and `Pyenv` address parts of 
Pyflow's *raison d'être*, but expose stumbling blocks that may frustrate new users, 
both when installing and using.  Some reasons why this is different:
 
- It automatically manages Python installations and environments. You specify a Python version
 in `pyproject.toml` (if ommitted, it asks), and ensures that version is used. 
 If the version's not installed, Pyflow downloads a binary, and uses that.
 If multiple installations are found for that version, it asks which to use.
 `Pyenv` can be used to install Python, but only if your system is configured in a certain way: 
 I don’t think expecting a user’s computer to compile Python is reasonable.

- By not using Python to install or run, it remains environment-agnostic. 
This is important for making setup and use as simple and decison-free as
 possible. It's especially important on Linux, where there may be several versions
of Python installed, with different versions and access levels. This avoids
complications, especially for new users. It's common for Python-based CLI tools
to not run properly when installed from `pip` due to the `PATH` or user directories
not being configured in the expected way. Pipenv’s installation 
instructions are confusing, and may result in it not working correctly.

- Its dependency resolution and locking is faster due to using a cached
database of dependencies, vice downloading and checking each package, or relying
on the incomplete data available on the [pypi warehouse](https://github.com/pypa/warehouse).
Pipenv’s resolution in particular may be prohibitively-slow on weak internet connections.

- It keeps dependencies in the project directory, in `__pypackages__`. This is subtle, 
but reinforces the idea that there's
no hidden state.

- It will always use the specified version of Python. This is a notable limitation in `Poetry`; Poetry
may pick the wrong installation (eg Python2 vice Python3), with no obvious way to change it.
Poetry allows projects to specify version, but neither selects, 
nor provides a way to select the right one. If it chooses the wrong one, it will 
install the wrong environment, and produce a confusing 
error message. This can be worked around using `Pyenv`, but neither the poetry docs 
nor error message provide guidance 
on this. This adds friction to the workflow and may confuse new users, as it occurs 
by default on popular linux distros like Ubuntu. Additionally, `pyenv's` docs are 
confusing: It's not obvious how to install it, what operating systems
it's compatible with, or what additional dependencies are required.

- Multiple versions of a dependency can be installed, allowing resolution
of conflicting sub-dependencies. (ie: Your package requires `Dep A>=1.0` and `Dep B`.
`Dep B` requires Dep `A==0.9`) There are many cases where `Poetry` and `Pipenv` will fail
to resolve dependencies. Try it for yourself with a few
 random dependencies from [pypi](https://pypi.org/); there's a good chance you'll
 hit this problem using `Poetry` or `Pipenv`. *Limitations: This will not work for
some compiled dependencies, and attempting to package something using this will
trigger an error.*


## My OS comes with Python, and Virtual environments are easy. What's the point of this?
Hopefully we're not replacing [one problem](https://xkcd.com/1987/) with [another](https://xkcd.com/927/).

Some people like the virtual-environment workflow - it requires only tools included 
with Python, and uses few console commands to create,
and activate and environments. However, it may be tedius depending on workflow:
The commands may be long depending on the path of virtual envs and projects,
and it requires modifying the state of the terminal for each project, each time
you use it, which you may find inconvenient or inelegant.

I think we can do better. This is especially relevant for new Python users
who don't understand venvs, or are unaware of the hazards of working with a system Python.
 
`Pipenv` improves the workflow by automating environment use, and 
allowing reproducable dependency graphs. `Poetry` improves upon `Pipenv's` API,
speed, and dependency resolution, as well as improving
the packaging and distributing process by using a consolidating project config. Both
 are sensitive to the Python environment used to run them, and won't work
 correctly if it's not as expected. 

`Conda` addresses these problems elegantly, but maintains a separate repository
of binaries from `PyPi`. If all packages you need are available on `Conda`, it may
be the best solution. If not, it requires falling back to `Pip`, which means 
using two separate package managers.

When building and deploying packages, a set of overlapping files are 
traditionally used: `setup.py`, `setup.cfg`, `requirements.txt` and `MANIFEST.in`. We use
`pyproject.toml` as the single-source of project info required to build
and publish.


## A thoroughly biased feature table
These tools have different scopes and purposes:

| Name | [Pip + venv](https://docs.python.org/3/library/venv.html) | [Pipenv](https://docs.pipenv.org) | [Poetry](https://poetry.eustace.io) | [pyenv](https://github.com/pyenv/pyenv) | [pythonloc](https://github.com/cs01/pythonloc) | [Conda](https://docs.conda.io/en/latest/) |this |
|------|------------|--------|--------|-------|-----------|-------|-----|
| **Manages dependencies** | ✓ | ✓ | ✓ | | | ✓ | ✓|
| **Manages Python installations** | | | | ✓ | | ✓ | ✓ |
| **Py-environment-agnostic** | | | | ✓ | | ✓ | ✓ |
| **Included with Python** | ✓ | | | | | | |
| **Stores packages with project** | | | | | ✓ | | ✓|
| **Locks dependencies** |  | ✓ | ✓ | | | ✓ | ✓|
| **Requires changing session state** | ✓ | | | ✓ | | | |
| **Slow** |  | ✓ | | | | | |
| **Easy script access** | | | | | | | ✓ |
| **Clean build/publish flow** | | | ✓ | | | | ✓ |
| **Supports old Python versions** | with `virtualenv` | ✓ | ✓ | ✓ | ✓ | ✓ | |


## Use
- Optionally, create a `pyproject.toml` file in your project directory. Otherwise, this
file will be created automatically. You may wish to use `pyproject new` to create a basic
project folder (With a .gitignore, source directory etc), or `pyproject init` to populate
info from `requirements.txt` or `Pipfile`. See
[PEP 518](https://www.python.org/dev/peps/pep-0518/) for details.

Example contents:
```toml
[tool.pyflow]
py_version = "3.7"
name = "runcible"
version = "0.1.0"
author = "John Hackworth"


[tool.pyflow.dependencies]
numpy = "^1.16.4"
diffeqpy = "1.1.0"
```
The `[tool.pyflow]` section is used for metadata. The only required item in it is
 `py_version`, unless
building and distributing a package. The `[tool.pyflow.dependencies]` section
contains all dependencies, and is an analog to `requirements.txt`. You can specify
developer dependencies in the `[tool.pyflow.dev-dependencies]` section. These
won't be packed or published, but will be installed locally.

You can specify `extra` dependencies, which will only be installed when passing
explicit flags to `pyflow install`, or when included in another project with the appropriate
 flag enabled. Ie packages requiring this one can enable with 
`pip install -e` etc.
```toml
[tool.pyflow.extras]
test = ["pytest", "nose"]
secure = ["crypto"]
```

If you'd like to an install a dependency with extras, use syntax like this:
```toml
[tool.pyflow.dependencies]
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

You can specify direct entry points to parts of your program using something like this in `pyproject.toml`:
```toml
[tool.pyflow]
# ...
scripts = { name = "module:function" }
```
Where you replace `name`, `function`, and `module` with the name to call your script with, the 
function you wish to run, and the module it's in respectively. This is similar to specifying 
scripts in `setup.py` for built packages. The key difference is that functions specified here 
can be run at any time,
without having to build the package. Run with `pyflow scriptname` to do this.

If you run `pyflow package` on on a package using this, the result will work like normal script
entry points for somone using the package, regardless of if they're using this tool.


## What you can do

### Managing dependencies:
- `pyflow install` - Install all packages in `pyproject.toml`, and remove ones not (recursively) specified.
If an environment isn't already set up for the version specified in `pyproject.toml`, sets one up. If
no version is specified, it asks you.
- `pyflow install requests` - If you specify one or more packages after `install`, those packages will 
be added to `pyproject.toml` and installed
- `pyflow install numpy==1.16.4 matplotlib>=3.1` - Example with multiple dependencies, and specified versions
- `pyflow uninstall requests` - Remove one or more dependencies

### Running REPL and Python files in the environment:
- `pyflow` - Run a Python REPL
- `pyflow main.py` - Run a python file
- `pyflow ipython`, `pyflow black` etc - Run a CLI tool like `ipython`, or a project function
 For the former, this must have been installed by a dependency; for the latter, it's specfied
under `[tool.pyflow]`, `scripts`
- `pyflow script myscript.py` - Run a one-off script, outside a project directory, with per-file
package management

### Building and publishing:
- `pyflow package` - Package for distribution (uses setuptools internally, and 
builds both source and wheel.)
- `pyflow package --features "test all"` - Package for distribution with features enabled, 
as defined in `pyproject.toml`
- `pyflow publish` - Upload to PyPi (Repo specified in `pyproject.toml`. Uses `Twine` internally.)

### Misc:
- `pyflow list` - Display all installed packages and console scripts
- `pyflow new projname` - Create a directory containing the basics for a project: 
a readme, pyproject.toml, .gitignore, and directory for code
- `pyflow init` - Create a `pyproject.toml` file in an existing project directory. Pull info from
`requirements.text` and `Pipfile` as required.
- `pyflow reset` - Remove the environment, and uninstall all packages
- `pyflow clear` - Clear the global cache of downloaded packages, eg in
 `~/.local/share/pyflow` (Linux) or `~\AppData\Roaming\pyflow` (Windows)
and the global cache of one-off script environments, in `~/.local/share/pyflow/script-envs`.
- `pyflow -V` - Get the current version of this tool
- `pyflow help` Get help, including a list of available commands


## How installation and locking work
Running `pyflow install` syncs the project's installed dependencies with those
 specified in `pyproject.toml`. It generates `pyflow.lock`, which on subsequent runs,
  keeps dependencies each package a fixed version, as long as it continues to meet the constraints
  specified in `pyproject.toml`. Adding a
package name via the CLI, eg `pyflow install matplotlib` simply adds that requirement before proceeding.
`pyflow.lock` isn't meant to be edited directly.
 
Each dependency listed in `pyproject.toml` is checked for a compatible match in `pyflow.lock`
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
- Installing from sources other than `pypi` (eg repos, paths)
- The lock file is missing some info like hashes
- Adding a dependency via the CLI with a specific version constraint, or extras.
- Packaging and publishing projects that use compiled extensions
- Dealing with multiple-installed-versions of a dependency that uses importlib
or dynamic imports
- Install Python on Mac

## Building and uploading your project to PyPi
In order to build and publish your project, additional info is needed in
`pyproject.toml`, that mimics what would be in `setup.py`. Example:
```toml
[tool.pyflow]
name = "everythingkiller"
py_version = "3.6"
version = "0.1.0"
author = "Fraa Erasmas"
author_email = "raz@edhar.math"
description = "Small, but packs a punch!"
homepage = "https://everything.math"
repository = "https://github.com/raz/everythingkiller"
license = "MIT"
keywords = ["nanotech", "weapons"]
classifiers = [
    "Topic :: System :: Hardware",
    "Topic :: Scientific/Engineering :: Human Machine Interfaces",
]
scripts = { activate = "jeejah:activate" }
python_requires=">=3.6"

package_url = "https://upload.pypi.org/legacy/"


[tool.pyflow.dependencies]
numpy = "^1.16.4"
manim = "0.1.8"
ipython = {version = "^7.7.0", extras=["qtconsole"]}


[tool.pyflow.dev-dependencies]
black = "^18.0"
```
`package_url` is used to determine which package repository to upload to. If ommitted, 
`Pypi test` is used (`https://test.pypi.org/legacy/`).

## Building this from source                      
If you’d like to build from source, [download and install Rust]( https://www.rust-lang.org/tools/install),
clone the repo, and in the repo directory, run `cargo build --release`.

Ie on Linux:
```bash
curl https://sh.rustup.rs -sSf | sh
git clone https://github.com/david-oconnor/pyflow.git
cd pyflow
cargo build --release
```

## Updating
If installed via `Cargo`, run `cargo install pyflow --force`.

## Contributing
If you notice unexpected behavior or missing features, please post an issue,
or submit a PR. If you see unexpected
behavior, it's probably a bug! Post an issue listing the dependencies that did
not install correctly.


## Why not to use this
- It's adding another tool to an already complex field.
- Most of the features here are already provided by a range of existing packages,
like the ones in the table above.
- The field of contributers is expected to be small, since it's written in a different language.
- Dependency managers like Pipenv and Poetry work well enough for many cases,
have dedicated dev teams, and large userbases.
- Conda in particular handles many things this does quite well.


## Dependency cache repo:
- [Github](https://github.com/David-OConnor/pydeps)
Example API calls: `https://pydeps.herokuapp.com/requests`, 
`https://pydeps.herokuapp.com/requests/2.21.0`. 
This pulls all top-level
dependencies for the `requests` package, and the dependencies for version `2.21.0` respectively.
There is also a `POST` API for pulling info on specified versions.
 The first time this command is run
for a package/version combo, it may be slow. Subsequent calls, by anyone,
should be fast. This is due to having to download and install each package
on the server to properly determine dependencies, due to unreliable information
 on the `pypi warehouse`.
 
 
## Python binary sources:
- Windows: [Python official Visual Studio package](https://www.nuget.org/packages/python/3.8.0-b4),
by Steve Dower.
- Ubuntu/Debian: Built on Ubuntu 18.04, using standard procedures.


## Gotchas
- Make sure `__pypackages__` is in your `.gitignore` file.
- You may need to set up IDEs to find packages in `__pypackages__`. If using PyCharm, 
using the tree on the left, right click `__pypackages__/3.x/lib`,
select `Mark directory as`, `Sources Root`.
- Make sure the `pyflow` binary is accessible in your path. If installing
via a `deb`, `msi`, or `Cargo`, this should be set up automatically.

# References
- [PEP 582 - Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
- [PEP 518 - pyproject.toml](https://www.python.org/dev/peps/pep-518/)
- [Semantic versioning](https://semver.org/)
- [PEP 440 -- Version Identification and Dependency Specification](https://www.python.org/dev/peps/pep-0440/)
- [Specifying dependencies in Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [Predictable dependency management blog entry](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html)
- [Blog on why Pyhon dependencies are hard to determine](https://dustingram.com/articles/2018/03/05/why-pypi-doesnt-know-dependencies/)
