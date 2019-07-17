# Py Packages

This tool attempts to implement 
[PEP 582 -- Python local packages directory](https://www.python.org/dev/peps/pep-0582/). 
It abstracts over commands used for creating
activating, modifying, and using virtual environments. Per PEP 582, dependencies
are stored in the project directory → `__pypackages__` → `3.7`(etc) → `lib`.
The virtual environment is created in the same diretory as `lib`.

This is a new project undergoing active development: expect breaking changes
in the near future. 

Python 3.3 or newer required.

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
[tool.pypackage.dependencies]
numpy = "^1.16.4"
django = "2.0.0"
```

For details on how to specify dependencies in this `Cargo.toml`-inspired 
[semvar](https://semver.org) format,
 reference
[this guide](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html).


## Example use

Managing dependencies:
- `pypackages install` - Install all packages in `pyproject.toml`, and remove ones not (recursively) specified
- `pypackages install toolz` - If you specify one or more packages after `install`, only those packages will be installed, 
and will be added to `pyproject.toml`.
- `pypackages install numpy==1.16.4 matplotlib>=3.1.` - Example with multiple dependencies, and specified versions
- `pypackages uninstall toolz` - Remove a dependency

Running REPL and Python files in the environment:
- `pypackages python` - Run a Python REPL
- `pypackages ipython` - Run an IPython (AKA Jupyter) REPL
- `pypackages python main.py` - Run a python file

Building and publishing:
- `pypackages package` - Package for distribution (uses setuptools internally, and 
builds both source and binary if applicable.)
- `pypackages publish` - Upload to PyPi (Rep specified in `pyproject.toml`. Uses `Twine` internally.)

Misc:
- `pypackages list` - Run `pip list` in the environment

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
This is a signifcant impact the usability of Python, especially for new users. 
IDEs like `PyCharm` abstract this away, but are a specific solution
to a general problem. See [this section of PEP 582](https://www.python.org/dev/peps/pep-0582/#id3).

Additionally, if multiple versions of Python are installed, verifying you're using
the one you want may be difficult.


## Why add another Python dependency manager?
`Pipenv` and `Poetry` both address this problem. Some reasons why this tools is different:

- It eeps dependencies in the project directory, in `__pypackages__`. It abstracts
over the virtual environment, and doesn't modify files outside
the project directory.

- By not requiring Python to install or run, it remains intallation-agnostic.
This is especially important on Linux, where there may be several versions
of Python installed, with different versions and access levels. This avoids
complications, especially for new users.

- This automates the build and publish process, in addition to managing
dependencies.


## What this doesn't do currently
- Check or resolve dependency conflicts

## Building and uploading your project to PyPi.
In order to build and publish your project, additional info is needed in
`pyproject.toml`, that mimics what would be in `setup.py`. Example:
```toml
[tool.pypackage]
name = "nicepackage"
version = "0.1.0"
author = "Fraa Erasmas"
description = "Does things"
homepage = "https://nicepackage.com"
repository = "https://github.com/everythingkiller/nicepackage"

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
git clone https://github.com/david-oconnor/pypackages.git
cd pypackages
cargo build --release

```

## Contributing
If you notice unexpected behavior or missing features, please post an issue,
or submit a PR.


## Gotchas
- Make sure the `pypackage` binary is accessible in your path. If installing
via `Cargo`, this should be set up automatically.
- Make sure `__pypackage__` and `.venv` are in your `.gitignore` file.