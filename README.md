# Py Packages

This library attempts to implement [PEP 582 -- Python local packages directory]
(Python local packages directory). It abstracts over commands used for creating
activating, modifying, and using virtual environments.


## Installation
There are 2 main ways to install this:
- Download the binary from (fill in), and add it to a location on the system path.
For example, place it under `/usr/bin` in linux, or `~\AppData\Local\Programs\Python\Python37\bin` in Windows.
- If you have `Rust` installed, the most convenient way is to 
run `cargo install pypackages`. If you don't, this will result in a large download
you may not want.

## Use
Create the file `Python.toml` in your project directory.

## Why?

Using the main python installation, especially one the operating system depends on
isn't a good way to use python packages. Virtual environments isolate dependencies
for each project, but are cumbersome to use. An example workflow:

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

## Why use this over existing projects
Pipenv and Poetry both address this problem directly. This project aims to keep
dependencies in the project directory, both in `__pypackages__`, and a `.venv`
folder, containing the virtual environment. It doesn't modify files outside
your project directory. It aims to be simple, fast, and easy to use.

## What this doesn't do (at least currently)

- Lock dependencies
- Check or resolve dependency conflicts


## Why Rust over Python
We'd like to avoid the ambiguity of which Python version is running commands
from this library, by providing a binary available on the system path. The downside
to this approach is that contributing to this project may be less accessible
to its users.


## Gotchas
- Make sure the `pypackages` binary is accessible in your path.
- Make sure `__pypackages__` and `.venv` are in your `.gitignore` file.