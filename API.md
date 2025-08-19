# Pyflow commands API

* [] `new` - Create a project folder with the basics file structure

    Usage: 

    ```
    pyflow new <path>
    ```


    Arguments:

    * [x] `--name` - Creates project with the given `<name>` inside `<path>` folder, so these identifiers can be different.
    * [] `--src` - Creates `src` folder instead of `<path>` folder.

* [] `init` - Create a `pyproject.toml` from [] requirements.txt, [] pipfile, [] setup.py, etc. 
    To be run inside project folder with these files.

* [] `add` - the 