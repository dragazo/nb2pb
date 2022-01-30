# `nb2pb`

[NetsBlox](https://netsblox.org/) is an educational block-based programming environment (based on [Snap!](https://snap.berkeley.edu/)) which has a focus on advanced CS topics such as distributed computing, cybersecurity, and the internet of things.
[PyBlox](https://github.com/dragazo/PyBlox) is an educational Python environment which supports most of the same features as NetsBlox, but which has students program in native (unrestricted) Python rather than the block-based language.
`nb2pb` is a rust crate that allows for the compilation of NetsBlox (`nb`) project files into PyBlox (`pb`) project files.

## How to Use

In addition to being a native rust crate available on crates.io, `nb2pb` also has (simplified) [Python bindings](https://pypi.org/project/nb2pb/), which are used by PyBlox.
As the python bindings are our main interest, they will be kept up to date as the rust crate evolves.

## Installation

To use `nb2pb` as a Python package, you can simply install it through pip.

```sh
pip install nb2pb
```

We build wheels for several operating systems and versions of python (`>=3.6`).
However, if there is not a wheel for your platform, pip will have to compile the rust crate from source, which will require installing [`cargo`](https://doc.rust-lang.org/cargo/getting-started/installation.html).
If this is the case, feel free to [submit an issue](https://github.com/dragazo/nb2pb/issues/new) including your operating system and processor architecture, and we can see if your system can be officially supported (without needing to be compiled by users) in the future.

## Building Wheels

To build a wheel on the local system for installed versions of CPython and PyPy, run the following command:

```sh
maturin build --release --cargo-extra-args="--features pyo3"
```
