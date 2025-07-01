# How to make the most of Sprocket doc

> [!CAUTION]
> This document describes the beta release of the `doc` command, which is currently exposed under the `dev` subcommand (i.e. `sprocket dev doc`).
> This functionality is experimental and may change  prior to a full release.

Sprocket is capable of rendering rich HTML documentation for any WDL workspace! However to make the most of this tool, there are certain documentation conventions which should be followed. This page describes practices that are optional, but that enhance the quality of the resulting documentation.

If you find any awkward rendering or unexpected behavior, refer to this document and see if a convention better addresses your specific use case.

## Homepage

We encourage you to customize the experience of your user documentation by writing a custom Markdown document which can be embedded at the root of your generated documentation. A Markdown file can be embedded in the homepage with a command line argument when generating the documentation. Every page contains links back to the homepage. If no homepage is provided, your users will be faced with an empty screen stating "There's nothing to see on this page".

Currently, we do not offer a way to include arbitrary assets, so unfortunately you cannot embed a custom logo or add pages other than a homepage. We're working on this feature though, so be sure to follow along with our development!

## Distributing generated documentation

The generated documentation directory (named `docs` by default) is completely self contained and can be moved, zipped, and shared without any of the raw WDL files it documents. Please let us know if you run into any issues while sharing your documentation.

## Custom themes

While it is technically possible to supply your own custom CSS styling, this capability is currently undocumented. We recommend you stick with the default styling at this point in time, but do let us know what kinds of customization you would like to see in future releases! 

## Using preamble comments for file-level documentation

To provide top-level documentation for a file, add a comment block before the `version` statement where each line starts with a double pound sign (i.e., `##`, which we term a "preamble comment"). These preamble comments will be rendered as Markdown above the generated table of contents on that file's dedicated page. For example:

```wdl
## # This is a header
##
## This is a paragraph with **bolding**, _italics_, and `code` formatting.

version 1.2

workflow foo {}
```

## v1.0 and v1.1 structs

The WDL specification does not offer a way to document structs prior to WDL v1.2, so the pages for them are rather limited. The pages for these structs will have a copy of the raw WDL definition.

## v1.2 structs

HTML documentation for structs defined in v1.2 WDL has not yet been implemented, so they are given the same treatment as pre-v1.2 structs.

## Meta entries with special handling

All meta entries will render in the final HTML documentation, but there are some special conventions we introduce. Each key below is expected to have a WDL `String` value.

`description`
: Every struct, task, and workflow `meta` section should have a `description` key. This description string can have Markdown formatting. The `description` string should be less than 140 characters or it will be clipped in some contexts.

`help`
: This text can be of any length. It is best practice to keep `description` short and put any additional text needed under the `help` key. Help strings can also be styled with Markdown.

`category`
: Workflows can have a `category` key which will group workflow pages on the left sidebar.

`external_help`
: This key should have a URL as its value (i.e. a valid hyperlink represented as a WDL `String`), and will be rendered as a button which will open a new tab or window visiting the link.

`warning`
: This text will be rendered in a special "warning box" to draw the attention of users.

## Parameters (inputs and outputs)

Each input and output to a workflow or task should be documented, but there is some flexibility in the specifics. To get the most out of `sprocket doc`, it is recommended that each instance of parameter documentation be a meta object. That object should have at least a `description` key. If a parameter has a `String` value for its meta entry instead of a meta object, that string value will be treated as if it were the `description` key of a meta object with no other entries.

### Inputs

Each entry in the `input` section of a task or workflow is expected to have a corresponding entry in the `parameter_meta` section. There is special handling for the `group` key of a meta object when used as documentation for an input:

- all inputs sharing the same `String` value for the `group` key will be rendered together in a dedicated table
- required inputs are _always_ rendered under the "Required Inputs" table and thus should _not_ have a `group` key (it will be ignored if present)
- the `Common` group of inputs will always come after the required inputs
- inputs without a `group` will be rendered under "Other Inputs" which will be the last input table
- the `Resource` group of inputs will immediately precede the "Other Inputs" table
- all other groups will render alphabetically between the `Common` table and the `Resource` table.

### Outputs

Outputs can be documented in one of two places: either in the task/workflow `meta` section under an `outputs` key or at the root of the `parameter_meta` section. To be compliant with the [Sprocket `MatchingOutputMeta` lint rule](https://docs.rs/wdl/latest/wdl/lint/index.html#lint-rules), you should document each output under an `outputs` key in the `meta` section and not include outputs anywhere in the `parameter_meta`.
