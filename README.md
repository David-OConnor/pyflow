# Py Packages

This tool attempts to implement 
[PEP 582 -- Python local packages directory](Python local packages directory). It abstracts over commands used for creating
activating, modifying, and using virtual environments.


## Installation
There are 2 main ways to install this:
- Download the binary from (fill in), and add it to a location on the system path.
For example, place it under `/usr/bin` in linux, or `~\AppData\Local\Programs\Python\Python37\bin` in Windows.
- If you have `Rust` installed, the most convenient way is to 
run `cargo install pypackages`.


## Use
- Create the file `Python.toml` in your project directory

Example contents:
```toml
[Python]
version = "3.7"

[dependencies]
numpy = "^1.16.4"
django = "^2.0.0"
```
- In a terminal, Run `pyprojects install` to install all dependencies in the `Python.toml`.
- Run `pyprojects` followed by the command you wish in order to run that
command inside the virtual environment. Eg `pyprojects python main.py`.

Additional commands:
- `pyprojects install numpy`: Install a specific package (`numpy` in this example), and add it to `Python.toml`.
- `pyprojects uninstall numpy`. Uninstall a package, and remove it from `Python.toml`.


## Why?

Using the main python installation, especially one the operating system depends on
isn't a good way to use python packages. Virtual environments isolate dependencies
for each project, and prevent damaging system python setups,
but are cumbersome to use. An example workflow:

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
This is a signifcant impact the usability of Python, especially for new users. IDEs like `PyCharm` abstract this away, but are a specific solution
to a general problem.

If [PEP 582](Python local packages directory) is impelemented, this tool
will become obsolete, but this isn't likely to happen in the near future.


## Why use this over existing projects
Pipenv and Poetry both address this problem directly. This project aims to keep
dependencies in the project directory, both in `__pypackages__`, and a `.venv`
folder, containing the virtual environment. It doesn't modify files outside
your project directory. It aims to be simple, fast, and easy to use.


## What this doesn't do (at least currently)

- Lock dependencies
- Check or resolve dependency conflicts


## Why Rust over Python?
We'd like to avoid the ambiguity of which Python version is executing commands
from this tool by providing a standalone binary. Eg, as an executable
python script, we may run into ambiguity over which python version is activating
the environment and installing dependencies. (System Python 2? Python 3? A different
version of Python 3? A user-installed Python? Super-user Python?) 

A downside
to this approach is that contributing to this project may be less accessible
to its users.


## Why create a new config format?
In the future, this tool may adopt or consolidate with
[pyproject.toml](https://poetry.eustace.io/docs/pyproject/) or
[Pipfile](https://github.com/pypa/pipfile), but we're starting
with a custom config.

`requirements.txt` doesn't include useful metadata, like the required Python version.


## Gotchas
- Make sure the `pypackages` binary is accessible in your path. If installing
via `Cargo`, this should be set up automatically.
- Make sure `__pypackages__` and `.venv` are in your `.gitignore` file.