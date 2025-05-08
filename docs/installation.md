# Installation

If you're looking for the latest stable version of the `sprocket` command line
tool, you can download it from any of the [package managers](#package-managers)
listed below. Otherwise, see the [build from source](#build-from-source) section
on how to obtain and build a copy of the source code.

## Package Managers

### Homebrew

::: warning Notice
While we'd like to make `sprocket` easily installable via [Homebrew], we're
waiting to surpass the [75 star
requirement](https://docs.brew.sh/Acceptable-Formulae#niche-or-self-submitted-stuff)
for Homebrew formulas. If you feel so inclined, help us get there by starring [the
repo](https://github.com/stjude-rust-labs/sprocket)!
:::

### Crates.io

Before you can build `sprocket`, you'll need to install [Rust]. We recommend
using [rustup] to accomplish this. Once Rust is installed, you can install the
latest version of `sprocket` by running the following command.

::: code-group

```shell
cargo install sprocket
```

:::

This will pull in the latest published version on [crates.io].


## Build From Source

Both the source code and the instructions to build the `sprocket` command line
tool are available on GitHub at
[`stjude-rust-labs/sprocket`](https://github.com/stjude-rust-labs/sprocket).

* The [releases](https://github.com/stjude-rust-labs/sprocket/releases) page
  contains all of the official releases for the project.
* If desired, you can install either the latest unpublished version (the code
  available on `main`) _or_ any experimental features by checking out the
  associated feature branch (`git checkout <branch-name>`).

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

[Homebrew]: https://brew.sh/
[Rust]: https://rust-lang.org/
[rustup]: https://rustup.rs/
[crates.io]: https://crates.io/crates/sprocket
