# Installation

If you're looking for the latest stable version of the `sprocket` command line
tool, you can either [download it from the release page](#download), [build it
from source](#build-from-source) (most common), get Sprocket through a [package
manager](#package-managers) (support still being added), or use
[Docker](#docker).

## Download

A pre-built binary for `sprocket` can be downloaded from the latest [release
entry on GitHub](https://github.com/stjude-rust-labs/sprocket/releases). Each
platform has different requirements regarding shared libraries that are expected
to be installed.

## Build From Source

There are also a number of options to build `sprocket` from source, including
pulling in the released source from [crates.io](#cratesio) or downloading the
source directly from [GitHub](#github). 

All methods for building `sprocket` from source require [Rust] and `cargo` to be
installed. We recommend using [rustup] to accomplish this. 

### Crates.io

You can use `cargo` to install the latest version of `sprocket` from
[crates.io].

```shell
cargo install sprocket
```

If desired, you can also check out a specific version of `sprocket`.

```shell
cargo install sprocket@0.13.0
```

### GitHub

Both the source code and the instructions to build the `sprocket` command line
tool are available on GitHub at [`stjude-rust-labs/sprocket`][github-src].

* The [releases][github-releases] page contains all of the official releases for
  the project.
* If desired, you can install either the latest unpublished version (the code
  available on `main`) _or_ any experimental features by checking out the
  associated feature branch (`git checkout <branch-name>`).

The simplest way is just to clone the repository and build the `main` branch,
which is expected to always contained a compilable and correct (though, perhaps
unreleased) version of Sprocket.

```shell
git clone git@github.com:stjude-rust-labs/sprocket.git
cd sprocket
cargo run --release
```

## Package Managers

Unfortunately, `sprocket` isn't available on any package managers yet. We expect
this to change as Sprocket gains more popularity and meets package manager
requirements for distribution.

### Homebrew

::: warning Notice
While we'd like to make `sprocket` easily installable via [Homebrew], we're
waiting to surpass the [75 star
requirement](https://docs.brew.sh/Acceptable-Formulae#niche-or-self-submitted-stuff)
for Homebrew formulas. If you feel so inclined, help us get there by starring [the
repo](https://github.com/stjude-rust-labs/sprocket)!
:::

### Other Package Managers

::: tip Note
If you know of other, community-maintained
packages for `sprocket`, please let us know by opening up [a pull
request](https://github.com/stjude-rust-labs/sprocket/pulls).
:::

## Docker

Every released version of `sprocket` is available through the GitHub Container
Registry.

```bash
docker run ghcr.io/stjude-rust-labs/sprocket:v0.13.0 -h
```

## Shell Completions

`sprocket` can generate command-line completion scripts for various shells,
allowing you to use tab completion for commands and arguments.

::: warning Warning
The `sprocket` command line tool is currently under active development and is not yet
considered stable. This means commands, flags, or arguments might change between
versions. **You will need to regenerate the shell completion script using the
steps below each time you update `sprocket`**.
:::

To generate a completion script, use the `completions` subcommand, specifying your shell:

::: code-group

```shell
sprocket completions <SHELL>
```

:::

Supported shells are: `bash`, `elvish`, `fish`, `powershell` and `zsh`.

### Enabling Completions

The exact steps to correctly enable shell completions depend on your specific
shell and how it's configured. Generally it involves two main steps:

1. Run the `sprocket completions <your shell>` command and redirect its standard output into a file,
   often somewhere in your home directory. For example, a Bash user might run:

::: code-group

```shell
sprocket completions bash > ~/.bash_completions/sprocket.bash
```

:::

2. Modify you shell's startup configuration file (e.g. `~/.bashrc`, `~/.zshrc`,
`~/.config/fish/config.fish`, PowerShell's `$PROFILE`, Elvish's
`~/.config/elvish/rc.elv`) to source the file you just created. Continuing the
Bash example, add this line to your `~/.bashrc`

::: code-group

```shell
source ~/.bash_completions/sprocket.bash
```

:::

[crates.io]: https://crates.io/crates/sprocket
[github-releases]: https://github.com/stjude-rust-labs/sprocket/releases
[github-src]: https://github.com/stjude-rust-labs/sprocket
[Homebrew]: https://brew.sh
[Rust]: https://rust-lang.org
[rustup]: https://rustup.rs