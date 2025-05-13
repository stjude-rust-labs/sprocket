# Configuration Files

The Sprocket configuration loader sources and merges values from a location specified by a command-line option, an environment variable, and common configuration locations, in that priority order.

## Configuration Options

### Command-line configuration

A configuration file can be specified at runtime on the command line using the `--config` argument.

### Environment Variable

The path to a configuration file can be specified via the environment variable `SPROCKET_CONFIG`.

### Current Working Directory

Sprocket will look for a `sprocket.toml` in the current working directory when the `sprocket` command runs.

### XDG_CONFIG_HOME

On systems that support the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/), such as many Linux distributions, Sprocket will attempt to read configuration from `XDG_CONFIG_HOME/sprocket/sprocket.toml`. Please note that the location of `XDG_CONFIG_HOME` is operating system dependent.

On MacOS, Sprocket will attempt to read the configuration from `$HOME/.config/sprocket/sprocket.toml`.

On Windows, Sprocket will attempt to read configuration from `%USERPROFILE%\AppData\Roaming`.

The platform-specific locations can also be found [here](https://docs.rs/dirs/latest/dirs/fn.config_dir.html). 

## Configuration Values

Running the command `sprocket config resolve` will print the effective configuration. The default configuration can be written out using the `sprocket config init` argument.

## Configuration Resolution

Atomic configuration values are overwritten by higher priority configuration files. List values are appended.

For example, if you specify the following in a configuration file in the current working directory.

```
[format]
indentation_size = 5

[check]
except = ['ContainerUri']
```

And the following in a configuration file pointed to by `SPROCKET_CONFIG`.

```
[format]
indentation_size = 3

[check]
except = ['SnakeCase']
```

The final, effective configuration will be:

```
[format]
indentation_size = 3

[check]
except = ['ContainerUri', 'SnakeCase']
```

Configuration resolution can be disabled by passing the `--skip-config-search` option on the command line. This will disable searching for and loading configuration files. The only configuration loaded will be that (if) specified by the `--config` command line argument.
