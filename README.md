# Py Packages

This tool attempts to implement 
[PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/). 
It abstracts over commands used for creating
activating, modifying, and using virtual environments. Per PEP 582, dependencies
are stored in the project directory → `__pypackages__` → `3.7`(etc) → `lib`.
The virtual environment is created in the same diretory as `lib`.

This is a new project undergoing active development: expect breaking changes
in the near future. 


## Installation
There are 2 main ways to install:
- Download the binary from (fill in), and add it to a location on the system path.
For example, place it under `/usr/bin` in linux, or `~\AppData\Local\Programs\Python\Python37\bin` in Windows.
- If you have `Rust` installed, the most convenient way is to 
run `cargo install pypackage`.


## Use
- Create a `pyproject.toml` file in your project directory. See
[PEP 518](https://www.python.org/dev/peps/pep-0518/) for details.

Example contents:
```toml
[tool.pypackage]
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

### Running REPL and Python files in the environment:
- `pypackage python` - Run a Python REPL
- `pypackage ipython` - Run an IPython (AKA Jupyter) REPL
- `pypackage python main.py` - Run a python file

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

This signifcant impacts the usability of Python, especially for new users. 
IDEs like `PyCharm` abstract this away, but are a specific solution
to a general problem. See [this section of PEP 582](https://www.python.org/dev/peps/pep-0582/#id3).

Additionally, if multiple versions of Python are installed, verifying you're using
the one you want may be difficult.

When building and deploying packages, a set of other, redudant files are 
traditionally used: `setup.py`, `setup.cfg`, and `MANIFEST.in`


## Why add another Python dependency manager?
`Pipenv` and `Poetry` both address this problem. Some reasons why this tool is different:

- It keeps dependencies in the project directory, in `__pypackages__`, and
doesn't modify files outside
the project directory.

- By not requiring Python to install or run, it remains intallation-agnostic.
This is especially important on Linux, where there may be several versions
of Python installed, with different versions and access levels. This avoids
complications, especially for new users. It's common for Python-based CLI tools
to not run properly when installed from `pip` due to the `PATH` 
not being configured in the expected way.

- If multiple python versions are found, it allows the user to select the desired 
one to set up the environment with.

`Conda` addresses this as well, but focuses on maintining a separate repository
of binaries from `PyPi`.


## Todo
- Check or resolve dependency conflicts
- Improve CLI and console feedback.
- Improve docs


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

## Contributing
If you notice unexpected behavior or missing features, please post an issue,
or submit a PR.


## Gotchas
- Make sure the `pypackage` binary is accessible in your path. If installing
via `Cargo`, this should be set up automatically.
- Make sure `__pypackages__` and `.venv` are in your `.gitignore` file.