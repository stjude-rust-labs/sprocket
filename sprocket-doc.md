# How to make the most of Sprocket doc

Sprocket is capable of rendering rich HTML documentation for any WDL workspace! However to make the most of this tool, there are certain documentation conventions which should be followed. This page describes practices that are optional, but that enhance the quality of the resulting documentation.

If you've already generated documentation for your workspace and have found any awkward rendering or unexpected behavior, you should refer here to see if there's a convention offered to better address your specific use case.

## Distributing generated documentation

The generated documentation directory (named `docs` by default) is completely self contained and can be moved, zipped, and shared without any of the raw WDL files it documents. Please let us know if you run into any issues while sharing your documentation!

## Custom themes

While it is technically possible to supply your own custom CSS styling, this capability is currently undocumented. We recommend you stick with the default styling at this point in time, but do let us know what kinds of customization you would like to see in future releases! 

## Using preamble comments for file-level documentation

To provide top-level documentation for a file, add a comment block before the `version` statement where each line starts with `##`. These preamble comments will be rendered as Markdown above the generated table of contents on that file's dedicated page. For example:

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

Structs defined in v1.2 WDL _will_ eventually get rich HTML documentation similar to tasks and workflows. However this has not yet been implemented, so they are given the same treatment as pre-v1.2 structs.

## Parameters (inputs and outputs)

Each input and output to a workflow or task should be documented, but there is some flexibility in the specifics.

### Inputs

Each entry in the `input` section of a task or workflow is expected to have a corresponding entry in the `parameter_meta` section. To get the most out of `sprocket doc`, it's recommended that each input entry be a meta object. That object should have at least a `description` key. There is special handling for the following keys:

- `group`: all inputs sharing the same `String` value will be rendered together in a dedicated table
    - required inputs are _always_ rendered under the "Required Inputs" table and thus should _not_ have a `group` key (it will be ignored if present)
    - The `Common` group of inputs will always come after the required inputs
    - Inputs without a `group` will be rendered under "Other Inputs" which will be the last input table
    - The `Resource` group of inputs will immediately precede the "Other Inputs" table
    - All other groups will render alphabetically between the `Common` table and the `Resource` table.
- `help`: will render at the top of the "Additional Meta" cell of the table

If an input has a `String` value for its parameter meta entry instead of a meta object, that string value will be treated as if it were the `description` key of a meta object.

### Outputs

Outputs can be documented in one of two places: either in the task/workflow `meta` section under an `outputs` key or at the root of the `parameter_meta` section. To be compliant with the [Sprocket `MatchingOutputMeta` lint rule](https://docs.rs/wdl/latest/wdl/lint/index.html#lint-rules), you should document each output under an `outputs` key in the `meta` section.

Similar to inputs, each output should either be documented with an object which has a `description` key with a `String` value, or a `String` value directly. 

## Meta entries with special handling

All meta entries will render in the final HTML documentation, but there are some special cases we introduce.

Every struct, task, and workflow `meta` section should have a `description` key with a `String` value. This description string can have Markdown formatting. The `description` string should be less than 140 characters or it will be clipped in some contexts.

The `help` key should only have a `String` value, but can be of any length. It's best practice to keep `description` short, and put any additional text needed under the `help` key. Help strings can also be styled with Markdown!

The `external_help` key should have a URL as its value, and will be rendered as a button which will open a new tab or window visiting the link.

If a `warning` key has a `String` value, it will be rendered in a special "warning box" to draw the attention of users.

Workflows can have a `category` key which will group workflow pages on the left sidebar.
