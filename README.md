[![crates.io version](https://meritbadge.herokuapp.com/pyflow)](https://crates.io/crates/pyflow)
[![Build Status](https://travis-ci.org/David-OConnor/pyflow.svg?branch=master)](https://travis-ci.org/David-OConnor/pyflow)


# Pyflow

#### *Simple is better than complex* - The Zen of Python

Pyflow manages Python installations and dependencies.

![Demonstration](https://raw.githubusercontent.com/david-oconnor/pyflow/master/demo.gif)

**Goals**: Make using and publishing Python projects as simple as possible. Actively
managing Python environments shouldn't be required to use dependencies safely. We're attempting
to fix each stumbling block in the Python workflow, so that it's as elegant
as the language itself.

You don't need Python or any other tools installed to use Pyflow.

It runs standalone scripts in their
own environments with no config, and project functions directly from the CLI.

It implements [PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
and [Pep 518 (pyproject.toml)](https://www.python.org/dev/peps/pep-0518/), and supports Python ≥ 3.4.  


## Installation
- **Windows** - Download and run 
[this installer](https://github.com/David-OConnor/pyflow/releases/download/0.1.8/pyflow-0.1.8-x86_64.msi).
Or, if you have [Scoop](https://scoop.sh) installed, run `scoop install pyflow`.

- **Ubuntu or Debian** - Download and run 
[this deb](https://github.com/David-OConnor/pyflow/releases/download/0.1.8/pyflow_0.1.8_amd64.deb).

- **Fedora, CentOs, RedHat, or older versions of SUSE** - Download and run 
[this rpm](https://github.com/David-OConnor/pyflow/releases/download/0.1.8/pyflow-0.1.8.x86_64.rpm).

- **A different Linux distro** - Download this 
[standalone binary](https://github.com/David-OConnor/pyflow/releases/download/0.1.8/pyflow)
 and place it somewhere accessible by the PATH. For example, `/usr/bin`.

- **Mac** - Install Rust: `curl https://sh.rustup.rs -sSf | sh`, then run 
`cargo install pyflow`. If able, please build from source using the instructions near the bottom of 
this page and PR a binary, to make this easier in the future.
 
- **With Pip** - Run `pip install pflow` Note that you still run with `pyflow`, and 
it doesn't matter which Python you use to install it.
 The linux install using this method is much larger than 
with the above ones, and it doesn't yet work with Mac.
 
 - **If you have [Rust](https://www.rust-lang.org) installed** - Run `cargo install pyflow`.


## Quickstart
- *(Optional)* Run `pyflow init` in an existing project folder, or `pyflow new projname` 
to create a new project folder. `init` imports data from `requirements.txt` or `Pipfile`; `new`
creates a folder with the basics.
- Run `pyflow install` in a project folder to sync dependencies with `pyproject.toml`, 
or add dependencies to it. 
This file will be created if it doesn't exist.
- Run `pyflow` or `pyflow myfile.py` to run Python.


## Quick-and-dirty start for quick-and-dirty scripts
- Add the line `__requires__ = [numpy, requests]` somewhere in your script, where `numpy` and 
`requests` are dependencies.
Run `pyflow script myscript.py`, where `myscript.py` is the name of your script.
This will set up an isolated environment for this script, and install
dependencies as required. This is a safe way
to run one-off Python files that aren't attached to a project, but have dependencies.


## Why add another Python manager?
`Pipenv`, `Poetry`, and `Pyenv` address parts of 
Pyflow's *raison d'être*, but expose stumbling blocks that may frustrate new users, 
both when installing and using. Some reasons why this is different:
  
- It behaves consistently regardless of how your system and Python installations
are configured.
  
- It automatically manages Python installations and environments. You specify a Python version
 in `pyproject.toml` (if ommitted, it asks), and it ensures that version is used. 
 If the version's not installed, Pyflow downloads a binary, and uses that.
 If multiple installations are found for that version, it asks which to use.
 `Pyenv` can be used to install Python, but only if your system is configured in a certain way: 
 I don’t think expecting a user’s computer to compile Python is reasonable.

- By not using Python to install or run, it remains environment-agnostic. 
This is important for making setup and use as simple and decison-free as
 possible. It's common for Python-based CLI tools
to not run properly when installed from `pip` due to the `PATH` or user directories
not being configured in the expected way.

- Its dependency resolution and locking is faster due to using a cached
database of dependencies, vice downloading and checking each package, or relying
on the incomplete data available on the [pypi warehouse](https://github.com/pypa/warehouse).
`Pipenv`’s resolution in particular may be prohibitively-slow on weak internet connections.

- It keeps dependencies in the project directory, in `__pypackages__`. This is subtle, 
but reinforces the idea that there's
no hidden state.

- It will always use the specified version of Python. This is a notable limitation in `Poetry`; Poetry
may pick the wrong installation (eg Python2 vice Python3), with no obvious way to change it.
Poetry allows projects to specify version, but neither selects, 
nor provides a way to select the right one. If it chooses the wrong one, it will 
install the wrong environment, and produce a confusing 
error message. This can be worked around using `Pyenv`, but this solution isn't 
documented, and adds friction to the 
workflow. It may confuse new users, as it occurs 
by default on popular linux distros like Ubuntu. Additionally, `Pyenv's` docs are 
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

Perhaps the biggest philosophical difference is that Pyflow abstracts over environments,
rather than expecting users to manage them.


## My OS comes with Python, and Virtual environments are easy. What's the point of this?
Hopefully we're not replacing [one problem](https://xkcd.com/1987/) with [another](https://xkcd.com/927/).

Some people like the virtual-environment workflow - it requires only tools included 
with Python, and uses few console commands to create,
and activate and environments. However, it may be tedious depending on workflow:
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
| **Resolves/locks deps** |  | ✓ | ✓ | | | ✓ | ✓|
| **Manages Python installations** | | | | ✓ | | ✓ | ✓ |
| **Py-environment-agnostic** | | | | ✓ | | ✓ | ✓ |
| **Included with Python** | ✓ | | | | | | |
| **Stores deps with project** | | | | | ✓ | | ✓|
| **Requires changing session state** | ✓ | | | ✓ | | | |
| **Clean build/publish flow** | | | ✓ | | | | ✓ |
| **Supports old Python versions** | with `virtualenv` | ✓ | ✓ | ✓ | ✓ | ✓ | |
| **Isolated envs for scripts** | | | | | | | ✓ |
| **Runs project fns from CLI** | | | | | | | ✓ |


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
version = "0.1.8"
authors = ["John Hackworth <jhackworth@vic.org>"]


[tool.pyflow.dependencies]
numpy = "^1.16.4"
diffeqpy = "1.1.0"
```
The `[tool.pyflow]` section is used for metadata. The only required item in it is
 `py_version`, unless
building and distributing a package. The `[tool.pyflow.dependencies]` section
contains all dependencies, and is an analog to `requirements.txt`. You can specify
developer dependencies in the `[tool.pyflow.dev-dependencies]` section. These
won't be packed or published, but will be installed locally. You can install these
from the cli using the `--dev` flag. Eg: `pyflow install black --dev`

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

To install from a local path instead of `pypi`, use syntax like this:
```toml
[tool.pyflow.dependencies]
# packagename = { path = "path-to-package"}
numpy = { path = "../numpy" }
```

To install from a `git` repo, use syntax like this:
```toml
[tool.pyflow.dependencies]
saturn = { git = "https://github.com/david-oconnor/saturn.git" }  # The trailing `.git` here is optional.
```

`git`dependencies are currently experimental. If you run into problems with them,
please submit an issue.

To install a package that includes a `.` in its name, enclose the name in quotes.

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
[tool.pyflow.scripts]
name = "module:function"
```
Where you replace `name`, `function`, and `module` with the name to call your script with, the 
function you wish to run, and the module it's in respectively. This is similar to specifying 
scripts in `setup.py` for built packages. The key difference is that functions specified here 
can be run at any time,
without having to build the package. Run with `pyflow name` to do this.

If you run `pyflow package` on on a package using this, the result will work like normal script
entry points for somone using the package, regardless of if they're using this tool.


## What you can do

### Managing dependencies:
- `pyflow install` - Install all packages in `pyproject.toml`, and remove ones not (recursively) specified.
If an environment isn't already set up for the version specified in `pyproject.toml`, sets one up. If
no version is specified, it asks you.
- `pyflow install requests` - If you specify one or more packages after `install`, those packages will 
be added to `pyproject.toml` and installed. You can use the `--dev` flag to install dev dependencies. eg:
`pyflow install black --dev`.
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
- `pyflow package --extras "test all"` - Package for distribution with extra features enabled, 
as defined in `pyproject.toml`
- `pyflow publish` - Upload to PyPi (Repo specified in `pyproject.toml`. Uses `Twine` internally.)

### Misc:
- `pyflow list` - Display all installed packages and console scripts
- `pyflow new projname` - Create a directory containing the basics for a project: 
a readme, pyproject.toml, .gitignore, and directory for code
- `pyflow init` - Create a `pyproject.toml` file in an existing project directory. Pull info from
`requirements.text` and `Pipfile` as required.
- `pyflow reset` - Remove the environment, and uninstall all packages
- `pyflow clear` - Clear the cache, of downloaded dependencies, Python installations, or script-
environments; it will ask you which ones you'd like to clear.
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
- Installing global CLI tools
- The lock file is missing some info like hashes
- Adding a dependency via the CLI with a specific version constraint, or extras.
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
version = "0.1.8"
authors = ["Fraa Erasmas <raz@edhar.math>"]
description = "Small, but packs a punch!"
homepage = "https://everything.math"
repository = "https://github.com/raz/everythingkiller"
license = "MIT"
keywords = ["nanotech", "weapons"]
classifiers = [
    "Topic :: System :: Hardware",
    "Topic :: Scientific/Engineering :: Human Machine Interfaces",
]
python_requires = ">=3.6"
# If not included, will default to `test.pypi.org`
package_url = "https://upload.pypi.org/legacy/"


[tool.pyflow.scripts]
# name = "module:function"
activate = "jeejah:activate"


[tool.pyflow.dependencies]
numpy = "^1.16.4"
manimlib = "0.1.8"
ipython = {version = "^7.7.0", extras=["qtconsole"]}


[tool.pyflow.dev-dependencies]
black = "^18.0"
```
`package_url` is used to determine which package repository to upload to. If ommitted, 
`Pypi test` is used (`https://test.pypi.org/legacy/`).

Other items you can specify in `[tool.pyflow]`:
- `readme`: The readme filename, use this if it's named something other than `README.md`.
- `build`: A python script to execute building non-python extensions when running `pyflow package`.

## Building this from source                      
If you’d like to build from source, [download and install Rust]( https://www.rust-lang.org/tools/install),
clone the repo, and in the repo directory, run `cargo build --release`.

Ie on linux or Mac:
```bash
curl https://sh.rustup.rs -sSf | sh
git clone https://github.com/david-oconnor/pyflow.git
cd pyflow
cargo build --release
```

## Updating
- If installed via `Scoop`, run `scoop update pyflow`.
- If installed via `Snap`, run `snap refresh pyflow`.
- If installed via `Cargo`, run `cargo install pyflow --force`. 
- If installed via `Pip`, run `pip install --upgrade pflow`.
- If using an installer or 
deb, run the new version's installer or deb. If manually calling a binary, replace it.

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
- `Conda` in particular handles many things this does quite well.


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
### [Repo binaries are downloaded from](https://github.com/David-OConnor/pybin/releases)
- Windows: [Python official Visual Studio package](https://www.nuget.org/packages/python/3.8.0-b4),
by Steve Dower.
- Newer linux distros: Built on Ubuntu 18.04, using standard procedures.
- Older linux distros: Built on CentOS 7, using standard procedures.


## Gotchas
- Make sure `__pypackages__` is in your `.gitignore` file.
- You may need to set up IDEs to find packages in `__pypackages__`. If using PyCharm:
`Settings` → `Project` → `Project Interpreter` → `⚙` → `Show All...` → 
(Select the interpreter, ie `(projname)/__pypackages__/3.x/.venv/bin/python`) → 
Click the folder-tree icon at the bottom of the pop-out window →
 Click the `+` icon at the bottom of the new pop-out window →
 Navigate to and select `(projname)/__pypackages__/3.x/lib`
- If using VsCode: `Settings` → search `python extra paths` →
 `Edit in settings.json` → Add or modify the line: 
 `"python.autoComplete.extraPaths": ["(projname)/__pypackages__/3.7/lib"]`


# References
- [PEP 582 - Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
- [PEP 518 - pyproject.toml](https://www.python.org/dev/peps/pep-518/)
- [Semantic versioning](https://semver.org/)
- [PEP 440 -- Version Identification and Dependency Specification](https://www.python.org/dev/peps/pep-0440/)
- [Specifying dependencies in Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [Predictable dependency management blog entry](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html)
- [Blog on why Pyhon dependencies are hard to determine](https://dustingram.com/articles/2018/03/05/why-pypi-doesnt-know-dependencies/)
