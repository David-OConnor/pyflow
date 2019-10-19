from setuptools import setup, find_packages

with open('README.rst') as f:
    readme = f.read()

setup(
    name="pflow",
    version="0.1.7",
    packages=find_packages(),

    install_requires=[],

    author="David O'Connor",
    author_email="david.alan.oconnor@gmail.com",
    url='https://github.com/David-OConnor/pyflow',
    description="A Python installation and dependency manager",
    long_description=readme,
    license="MIT",
    keywords="packaging, dependencies, install",
)
