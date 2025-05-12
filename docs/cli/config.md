# Configuration Files

The Sprocket configuration loader sources from command-line options, environment variables, and configuration files, in that priority order.

## Configuration Options

### Command-line configuration

A configuration file can be specified at runtime on the command line using the `--config` argument.

### Environment Variable

The path to a configuration file can be specified via the environment variable `SPROCKET_CONFIG`.

### Current Working Directory

Sprocket will look for a `sprocket.toml` in the current working directory when the `sprocket` command runs.

### XDG_CONFIG_HOME

Sprocket will attempt to read a configuration file from `XDG_CONFIG_HOME/sprocket/sprocket.toml`. The location of `XDG_CONFIG_HOME` is operating system dependent. The platform-specific values can be found [here](https://docs.rs/dirs/latest/dirs/fn.config_dir.html). On MacOS, Sprocket will attempt to read the configuration from `$HOME/.config/sprocket/sprocket.toml`.

## Configuration Values

Running the command `sprocket config` will print the effective configuration. The default configuration can be written out using the `--generate` argument.