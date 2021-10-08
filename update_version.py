# A script to update the version in config files.
import re
import sys

vers = sys.argv[1]


def helper(filename: str, startswith: str, quotes: bool):
    data = ""
    with open(filename) as f:
        for line in f.readlines():
            if line.startswith(startswith):
                vers2 = f'"{vers}"' if quotes else vers
                data += startswith + vers2 + "\n"
            else:
                data += line
    with open(filename, 'w') as f:
        f.write(data)


def main():
    helper('Cargo.toml', "version = ", True)
    helper('snapcraft.yaml', "version: ", False)

    data = ""
    with open('README.md') as f:
        for line in f.readlines():
            line = re.sub(r'0\.\d\.(\d{1,3})', vers, line)
            data += line

    with open('README.md', 'w') as f:
        f.write(data)

    print(f"Updated version to {vers}")


main()
