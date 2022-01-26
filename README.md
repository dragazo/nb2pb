## Publishing to PyPi

`pyo3` and `maturin` create native python modules, which means you need to build on each platform you want to target.
Docker allows us to do all of this on one (physical) machine.
The following instructions assume you are on a linux machine and have `docker` installed.

- Build for linux

    ```sh
    sudo docker run --rm -v $(pwd):/io konstin2/maturin build --release
    ```

- Build for windows
- Build for mac
