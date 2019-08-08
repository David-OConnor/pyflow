# Py Packages

This tool implements
[PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/). 
It manages dependencies, keeping them isolated in the project directory, and runs
python in an environment which uses this directory. Per PEP 582, dependencies
are stored in the project directory → `__pypackages__` → `3.7`(etc) → `lib`.
A virtual environment is created in the same diretory as `lib`, and is used
transparently.

This is a new project undergoing active development: expect breaking changes
in the near future. 

Python ≥ 3.3 is required.


## Installation
There are 2 main ways to install:
- Download a binary from the `releases` page. On Debian or Ubuntu, download and run
[This deb](https:). 

On other Operating systems, download the appropriate binary, and place it somewhere
accessible by the system path. For example, place it under `/usr/bin` in linux, 
or `~\AppData\Local\Programs\Python\Python37\bin` in Windows.

- If you have `Rust` installed, the most convenient way is to 
run `cargo install pypackage`.


## Use
- Create a `pyproject.toml` file in your project directory. See
[PEP 518](https://www.python.org/dev/peps/pep-0518/) for details.

Example contents:
```toml
[tool.pypackage]
py_version = "3.7"
name = "runcible"
version = "0.1.0"
author = " John Hackworth"


[tool.pypackage.dependencies]
numpy = "^1.16.4"
diffeqpy = "1.1.0"
```
The `[tool.pypackage]` section is used for metadata, and isn't required unless
building and distributing a package. The `[tool.pypyackage.dependencies'`] section
contains all dependencies, and is an analog to `requirements.txt`. For details on 
how to specify dependencies in this `Cargo.toml`-inspired 
[semvar](https://semver.org) format,
 reference
[this guide](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html).


## Example use

### Managing dependencies:
- `pypackage install` - Install all packages in `pyproject.toml`, and remove ones not (recursively) specified
- `pypackage install toolz` - If you specify one or more packages after `install`, only those packages will be installed, 
and will be added to `pyproject.toml`.
- `pypackage install numpy==1.16.4 matplotlib>=3.1.` - Example with multiple dependencies, and specified versions
- `pypackage uninstall toolz` - Remove a dependency
- `pypackage install ipython -b` - Install a dependency which includes a binary script. 
(eg `ipython`, `black`, `mypy`, `poetry` etc). These won't work properly if you don't include 
the `-b` or `--bin` flag.


### Running REPL and Python files in the environment:
- `pypackage python` - Run a Python REPL
- `pypackage python main.py` - Run a python file
- `pypackage black`, `pypackage ipython` etc - Run a custom binary, if it's installed.

### Building and publishing:
- `pypackage package` - Package for distribution (uses setuptools internally, and 
builds both source and binary if applicable.)
- `pypackage publish` - Upload to PyPi (Rep specified in `pyproject.toml`. Uses `Twine` internally.)

### Misc:
- `pypackage new projname` - Create a directory containing the basics for
a project: a readme, pyproject.toml, and directory for source code
- `pypackage list` - Run `pip list` in the environment
- `pypackage version` - Get the current version of this tool
- `pypackage help` Get help, including a list of available commands

## How dependencies are resolved
Running `pypackage install` loads the project's requirements from `pyproject.toml`. Adding a
package name via the CLI, eg `pypackage install matplotlib` simply adds that requirement before proceeding.
Compatible versions of dependencies are determined using info from 
the [PyPi Warehosue](https://github.com/pypa/warehouse) (available versions, and hash info), 
and the `pydeps` database. We use `pydeps`, which is built specifically for this project,
due to inconsistent dependency information stored on `pypi`. A dependency graph is built
using this cached database. We attempt to use the newest compatible version of each package,
but older ones are used if needed to satisfy the dependency occuring with different requirements.

This tool downloads and unpacks wheels from `pypi`, or builds
wheels from source if none are availabile. It verifies hashes using `SHA256`, and the exact 
versions usedare stored 
in a lock file.

If a lockfile already exists, package versions stored in it which are compatible with those
in `pyproject.toml` and resolved subdependencies are used.

Important caveat: There appears to be no way install multiple versions of a package
simultaneously without renaming them; this is a factor when encountering incompatible sub-dependencies.
Perhaps this can be sorted around through behind-the-scenes renaming and import-line
edits, but for now may result in unresolvable trees.

## Why?
Using a Python installation directly when installing dependencies can become messy.
If using a system-level Python, which is ubiqutious in Linux, altering dependencies
may break the OS. Virtual environments correct this, but are cumbersome to use. 
An example workflow:

Setup:
```bash
cd ~/.virtualenvs
python -m venv "myproject"
cd myproject/bin
source activate
cd ~/myproject
python install -r requirements.txt
deactivate
```
Use:
```bash
cd ~/.virtualenvs/myproject/bin
source activate
cd ~/myproject
python main.py
deactivate
```

This signifcantly impacts the usability of Python, especially for new users. 
IDEs like `PyCharm` abstract this away, but are a specific solution
to a general problem. See [this section of PEP 582](https://www.python.org/dev/peps/pep-0582/#id3).

If multiple versions of Python are installed, verifying you're using
the one you want may be difficult.

When building and deploying packages, a set of other, redudant files are 
traditionally used: `setup.py`, `setup.cfg`, and `MANIFEST.in`


## Why add another Python dependency manager?
`Pipenv` and `Poetry` both address this problem. Some reasons why this tool is different:

- It keeps dependencies in the project directory, in `__pypackages__`, and
doesn't modify files outside the project directory.

- It doesn't use Pip.

- Its dependency resolution and locking is faster due to using a cached
database of dependencies, vice downloading and checking each package, or relying
on the incomplete data available on the `pypi warehouse`.

- Multiple versions of a dependency can be installed, allowing resolution
of conflicting sub-dependencies.

- By not requiring Python to install or run, it remains intallation-agnostic.
This is especially important on Linux, where there may be several versions
of Python installed, with different versions and access levels. This avoids
complications, especially for new users. It's common for Python-based CLI tools
to not run properly when installed from `pip` due to the `PATH` 
not being configured in the expected way. Rust is both a natural choice for making
a standalone CLI app, and an example of well-designed dependency management.

- If multiple Python installations are found, it allows the user to select the desired 
one to set up the environment with.

`Conda` addresses this as well, but focuses on maintining a separate repository
of binaries from `PyPi`.


## Not-yet-implemented
- Installing multiple versions of a sub-dependency when required
- Binary/script installations (Important feature!)
- Installing extra dependencies for features
- `SHA256` hashing vice the less-secure `MD5`
- The lock file is missing some info like dependencies.


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

[tool.pypackage.dependencies]
numpy = "^1.16.4"
django = "2.0.0"
```

## Building this from source                      
If you’d like to build from source, [download and install Rust]( https://www.rust-lang.org/tools/install),
clone the repo, and in the repo directory, run `cargo build –release`.

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
or submit a PR. There are likely many problems with the dependency resolver
due to the complicated history of Python package management. If you see unexpected
behavior, it's probably a bug! Post an issue listing the dependencies that did
not install correctly.

## Dependency cache repo:
- [Github](https://github.com/David-OConnor/pydeps)
Example API call: `https://pydeps.herokuapp.com/numpy`. This pulls all top-level
dependencies for the `numpy` package. The first time this command is run
for a package/version combo, it may be slow. Subsequent calls, by anyone,
should be faster. This is due to having to download and install each package
on the server to properly determine dependencies, due to unreliable information
 on the `pypi warehouse`.


## Gotchas
- Make sure the `pypackage` binary is accessible in your path. If installing
via `Cargo`, this should be set up automatically.
- Make sure `__pypackages__` and `.venv` are in your `.gitignore` file.

# References
- [PEP 582 - Python local packages directory](https://www.python.org/dev/peps/pep-0582/)
- [Pep 518 - pyproject.toml](https://www.python.org/dev/peps/pep-518/)
- [Semantic versioning](https://semver.org/)
- [Specifying dependencies in Cargo](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
- [Predictable dependency management blog entry](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html)
- [Blog on why Pyhon dependencies are hard to determine](https://dustingram.com/articles/2018/03/05/why-pypi-doesnt-know-dependencies/)