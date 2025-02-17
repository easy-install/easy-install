# easy-install: A cross-platform CLI installation tool based on github release

Typically, CLI tools are installed on different platforms using package managers such as `apt`, `brew`, `winget`, `choco`, `pacman`, etc. However, when testing various CLI tools in GitHub CI, the process becomes very tedious. You end up having to run different commands on each platform, and sometimes tools aren’t even available in app stores. This means you have to download an archive, extract the files, and add the binary to your `$PATH`.

Some projects try to simplify this process by providing an `install.sh` script. But with Windows in the mix, you still need to handle platform-specific differences.

That’s why I created **easy-install**—a CLI installation tool based on GitHub releases. If your CLI tool follows the cargo-binstall or cargo-dist release format (i.e., naming the artifacts as `{name}-{target}`), you can install it with a single command:

```bash
ei https://github.com/denoland/deno
```
Of course, there are many cases where you might want to install CLI tools that aren’t written in Rust, such as those implemented in C or Go. In such cases, you can write a cargo-dist formatted JSON file. For example:

```json
{
  "artifacts": {
    "https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-linux-x86_64": {
      "name": "qjs",
      "target_triples": ["x86_64-unknown-linux-gnu"]
    }
  }
}
```
Then, you can install the corresponding program using this resource configuration file:

```bash
ei quickjs-ng.json
```

Alternatively, you can create a build repository for older tools that don’t offer GitHub releases. Using CI, these tools can be compiled and uploaded following the {name}-{target} naming convention.

## Why Not Use apt/brew?
Using traditional package managers requires you to handle different installation methods across platforms. For instance, consider a scenario where you want to run tests to compare the performance of the quickjs runtime from quickjs-ng:

quickjs-ng (C implementation): You need to download the appropriate file for each platform, set execution permissions, and add it to $PATH.
txiki.js (C implementation): You must download the corresponding archive on each platform, extract it, set permissions, and update the $PATH.
llrt (Rust implementation): Similarly, you’d have to download, extract, set permissions, and adjust the $PATH.
This multi-step, platform-specific process quickly becomes cumbersome—especially when comparing performance across multiple JS engines and runtimes.

## Simplifying with easy-setup
Enter easy-setup, which leverages easy-install to drastically reduce the complexity in your GitHub Actions. With a simple configuration, you can install multiple tools across different platforms. Here’s an example configuration:

```yml
strategy:
  matrix:
    os: [ubuntu-24.04, windows-latest, macos-14, macos-13]
runs-on: ${{ matrix.os }}
steps:
  - uses: ahaoboy/easy-setup@v1
    with:
      url: |-
        https://github.com/ahaoboy/txiki.js-build
        https://github.com/ahaoboy/easy-install/raw/refs/heads/main/dist-manifest/llrt.json
        https://github.com/ahaoboy/easy-install/raw/refs/heads/main/dist-manifest/quickjs-ng.json
  - name: test
    run: |
      which qjs
      which llrt
      which tjs
```

This setup makes it incredibly simple to run tests across different operating systems without the hassle of managing multiple platform-specific installation commands.



## Test release

We usually run test before releasing, but this has a potential risk. The development environment will install many dependencies when the program is compiled. These dependencies may not exist when the user uses it. That is, the program can pass the test, but when the user downloads it from the release and uses it, the program cannot run.

Using easy-setup, you can easily add release tests as a final guarantee


For example, we add an action after release to test whether the latest release can work properly.

```yml
test-release:
  needs: ["build", "test", "release"]
  strategy:
    matrix:
      os: [ubuntu-24.04, windows-latest, macos-14, macos-13]
  runs-on: ${{ matrix.os }}
  steps:
    - uses: actions/checkout@v4
    - uses: easy-install/easy-setup@v1
      with:
        url: https://github.com/ahaoboy/mujs-build
    - name: test
      run: |
        which mujs
        echo "console.log(1+1)" >> ./test.js
        mujs ./test.js
```

If you’re interested or giving it a try, check out the following repositories:

https://github.com/easy-install/easy-install

https://github.com/easy-install/easy-setup