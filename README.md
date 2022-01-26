## Publishing to PyPi

Perform the following steps to generate wheels for various platforms and upload all of them with `twine upload ...`.
Note that some of these steps produce multiple versions of source distributions - the version uploaded to pypi should not matter.

- Build for Linux, Windows, and MacOS (`x86_64`)

    Manually run the main github CI action (will be automated once the project is in a more stable state)

- Build for MacOS (`M1`)

    Unfortunately must actually be performed on an M1 mac machine.

    ```sh
    maturin build --release
    ```
