## install

```bash
cargo binstall easy-install

cargo install easy-install
```

## usage

```bash
# install latest
ei https://github.com/ahaoboy/mujs-build

# install v0.0.1
ei https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.1

# install crate
ei ansi2


# install deno and denort
ei https://github.com/denoland/deno

# install deno
ei https://github.com/denoland/deno/releases/download/v2.1.1/deno-x86_64-pc-windows-msvc.zip
ei https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip
```


## dist-manifest.json

When the release package contains many files, add a dist-manifest.json file to define the format of each file

[cargo-dist-schema](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist-schema)


Taking mujs as an example, it contains the following files


```
.
├── libmujs.a
├── libmujs.o
├── libmujs.so
├── mujs-pp.exe
├── mujs.exe
└── mujs.pc

```


[dist-manifest.json](https://github.com/ahaoboy/mujs-build/blob/main/dist-manifest.json)

```
"mujs-aarch64-apple-darwin.tar.gz": {
  "name": "mujs-aarch64-apple-darwin.tar.gz",
  "target_triples": [
    "aarch64-apple-darwin"
  ],
  "assets": [
    {
      "name": "mujs",
      "path": "mujs",
      "kind": "executable"
    },
    {
      "name": "mujs-pp",
      "path": "mujs-pp",
      "kind": "executable"
    },
    {
      "name": "libmujs.dylib",
      "path": "libmujs.dylib",
      "kind": "c_dynamic_library"
    },
    {
      "name": "libmujs.a",
      "path": "libmujs.a",
      "kind": "c_static_library"
    }
  ]
},
```



```bash
ei "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.4/dist-manifest.json"
```

## zoo

- https://github.com/ahaoboy/mujs-build
- https://github.com/ahaoboy/quickjs-build
- https://github.com/ahaoboy/quickjs-ng-build
- https://github.com/ahaoboy/spidermonkey-build
- https://github.com/ahaoboy/v8-build
- https://github.com/ahaoboy/jsc-build