# Guided Tour

In this guide, we'll cover how to write, verify, and run WDL documents using
Sprocket. This should give you a good sense of where and how you might use
Sprocket in your day-to-day work.

To follow along, you are encouraged to download [this WDL
document](/guided-tour/example.wdl){target="_self" download="example.wdl"} that
has been specifically crafted for this walkthrough. Further, if you have not
already, please follow [this guide](./installation.md) to install Sprocket.

## Ensuring high-quality code

Using automation to ensure high-quality code both during development and through
continuous integration (CI) processes is one of the hallmarks of modern
development. Sprocket contains both the `check` and `lint` (shortcut for `check
--lint`) subcommands to help you during your WDL development.

### Exploring lint and validation rules

You can see a list of all of the rules Sprocket contains by running `sprocket
explain -h`. You can dive into any of the rules Sprocket includes by running
`sprocket explain <RULE>`. For example, if we wanted to learn more about the
`ImportSorted` rule, we could do the following.

```shell
sprocket explain ImportSorted
```

This command gives the following description of the `ImportSorted` rule.

```txt
ImportSorted [Style, Clarity, Sorting]
Ensures that imports are sorted lexicographically.

Imports should be sorted lexicographically to make it easier to find 
specific imports. This rule ensures that imports are sorted in a 
consistent manner. Specifically, the desired sort can be acheived
with a GNU compliant `sort` and `LC_COLLATE=C`. No comments are
permitted within an import statement.
```

We encourage you to explore the existing validation and linting rules supported
by Sprocket along with suggesting new helpful rules on our [issues
page](https://github.com/stjude-rust-labs/wdl/issues).

### Linting and validation

Both single WDL documents and directories of WDL documents can be validated and
linted by using the `sprocket lint` subcommand.

```shell
sprocket lint example.wdl
```

This returns a set of validation and linting diagnostics that can/should be
addressed by the workflow author. In this case, the following is the
output from the linting and validation process on `example.wdl`.

```txt
note[MetaSections]: task `say_hello` is missing both `meta` and `parameter_meta` sections
  ┌─ example.wdl:3:6
  │
3 │ task say_hello {
  │      ^^^^^^^^^ this task is missing both `meta` and `parameter_meta` sections
  │
  = fix: add both the `meta` and `parameter_meta` sections

note[ContainerUri]: container URI uses a mutable tag
   ┌─ example.wdl:18:20
   │
18 │         container: "ubuntu:latest"
   │                    ^^^^^^^^^^^^^^^
   │
   = fix: replace the mutable tag with its SHA256 equivalent (e.g., `ubuntu@sha256:foobar` instead of `ubuntu:latest`)

note[MetaSections]: workflow `main` is missing both `meta` and `parameter_meta` sections
   ┌─ example.wdl:22:10
   │
22 │ workflow main {
   │          ^^^^ this workflow is missing both `meta` and `parameter_meta` sections
   │
   = fix: add both the `meta` and `parameter_meta` sections

warning[UnusedInput]: unused input `color`
   ┌─ example.wdl:30:16
   │
30 │         String color = "green"
   │                ^^^^^
```

Specific lint rules can be ignored with multiple invocations of the `-e` flag.

```shell
sprocket lint example.wdl -e ContainerUri -e MetaSections
```

This leaves a single diagnostic, which is that `color` is an unused workflow
input. Before continuing on, we will remove that input from the `main` workflow
so it doesn't continue showing up in the diagnostics.

### Continuous integration

If you use GitHub for source control, you can use the [Sprocket GitHub
Action](https://github.com/stjude-rust-labs/sprocket-action) to ensure that your
WDL documents stay formatted correctly and free of validation/lint errors.

## Code editor integration

Rather than running `sprocket check` and `sprocket lint` continually on the
command line, most developers prefer to have these errors and lints show up in
their editor of choice. Sprocket makes available a language server protocol
(LSP) server under the `sprocket analyzer` command.

If you use Visual Studio Code, you can easily get started by simply installing
[the Sprocket VSCode
extension](https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode).
This automatically downloads the latest version of `sprocket` and integrates the
various lint and validation warnings into the "Problems" tab of your editor.

![A view of the "Problems" tab in VSCode with Sprocket reported
issues](./guided-tour/problems.png){style="margin-top: 30px;"}

## Running tasks and workflows

Individual tasks and workflows can be run with the `sprocket run` subcommand.

```shell
sprocket run example.wdl
```

This will error right away, as we haven't told Sprocket which task or workflow
to run.

```txt
error: the `--entrypoint` option is required if no inputs are provided
```

We want to run the "main" workflow defined in `example.wdl`, so we can try again
but specify the entrypoint to use this time using the `--entrypoint` flag.

```shell
sprocket run example.wdl --entrypoint main
```

After a few seconds, you'll see `sprocket` return an error.

```txt
error: failed to validate the inputs to workflow `main`

Caused by:
    missing required input `name` to workflow `main`
```

This is because the workflow has a required input parameter called `name` that
must be provided before the workflow can run. 

::: tip Understanding inputs

Inputs to a Sprocket run are provided as arguments passed after the WDL document
name is provided. Each input can be specified as either

* a key value pair (e.g., `main.is_pirate=true`)
* a JSON file containing inputs (e.g., a `defaults.json` file where the
  contents are `{ "main.is_pirate": true }`)
* a YAML file containing inputs (e.g. a `defaults.yaml` file where the
  contents are `main.is_pirate: true`)

Inputs are _incrementally_ applied, meaning that inputs specified later override
inputs specified earlier. This enables you to do something like the following to
use a set of default parameters and iterate through sample names in Bash rather
than create many individual input files.

```bash
sprocket run example.wdl defaults.json main.name="Ari"
```

Note that the above command does not specify an entrypoint with the `--entrypoint`
flag. This is because every input is using fully qualified dot notation; each
input is prefixed with the name of the entrypoint and a period, `main.`.
This fully qualified dot notation is required for inputs provided within a file.
The dot notation can get repetitive if supplying many key value pairs on the command line,
so specifying `--entrypoint` allows you to omit the repeated part of the keys.
:::

Here, we can specify the `name` parameter as a key-value pair on the command
line.

```shell
sprocket run example.wdl --entrypoint main name="World"
```

After a few seconds, this job runs successfully with the following outputs.

```json
{
  "main.messages": [
    "Hello, World!",
    "Hallo, World!",
    "Hej, World!"
  ]
}
```

Congrats on your first successful `sprocket run` 🎉!

If you wanted to override some of the defaults for the workflow, you could do
so by defining the input in a `overrides.json` file:

```json
{
  "main.greetings": [
    "Good morning",
    "Good afternoon",
    "Good evening"
  ],
  "main.is_pirate": true
}
```
and the providing that in the set of inputs to the workflow.

```shell
sprocket run example.wdl overrides.json main.name="Sprocket"
```

This produces the following output.

```json
{
  "main.messages": [
    "Good morning, Sprocket!",
    "Good afternoon, Sprocket!",
    "Good evening, Sprocket!",
    "Ahoy, Sprocket!"
  ]
}
```

## Conclusion

You should now have a clear idea on how the most commonly used commands within
Sprocket work. With further questions or feature requests, we ask that you join
the [#sprocket channel on the WDL
Slack](https://join.slack.com/t/openwdl/shared_invite/zt-ctmj4mhf-cFBNxIiZYs6SY9HgM9UAVw)
or file an issue on [the Sprocket
repository](https://github.com/stjude-rust-labs/sprocket/issues).