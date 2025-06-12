# How to make the most of Sprocket doc

Sprocket is capable of rendering rich HTML documentation for any WDL workspace! However to make the most of this tool, there are certain documentation conventions which should be followed. Of course you are free to ignore this document, and Sprocket should do a passable job of documenting your WDL, but there may be some awkward rendering or unexpected behavior if you aren't conforming to what the tool expects.

## General structure

The generated documentation directory (named `docs` by default) is completely self contained and can be moved, zipped, and shared without any of the raw WDL files it documents. Please let us know if you run into any issues while sharing your documentation!

The `docs` directory will always contain a `style.css` file, an `index.js` file, an `index.html` file, and an `assets/` directory filled with SVG icons and logos. The rest of the contents are dynamically created based on the structure of the WDL workspace being documented. Each WDL file found in the workspace will be represented as a directory within `docs`. The relative path from the root of the workspace to each WDL file is reproduced in the `docs` directory to keep your documentation organized.

The directory for each WDL file will have an `index.html` file and one HTML file per struct, task, or workflow defined in the WDL.

## Custom themes

While it is technically possible to supply your own custom CSS styling, this capability is currently undocumented. We recommend you stick with the default styling at this point in time, but do let us know what kinds of customization you would like to see in future releases! 

## Per WDL file `index.html`

At a minimum, the `index.html` associated with each WDL file will have a table of contents with links to each struct, task, or workflow documentation page in that directory. If the WDL file has a "preamble", that will be rendered as Markdown text above the table of contents.

### Preamble

Preamble comments are special comments at the start of a WDL file that begin with double pound signs (`##`). These comments are used for documentation that doesn't fit within any of the WDL-defined documentation elements (i.e., `meta` and `parameter_meta` sections). They may provide context for a collection of tasks or structs, or they may provide a high-level overview of a workflow.

Example:

```wdl
## # This is a header
##
## This is a paragraph with **bolding**, _italics_, and `code` formatting

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

Each entry in the `input` section of a task or workflow is expected to have a corresponding entry in the `parameter_meta` section. To get the most out of `sprocket doc`, it's recommended that each input entry be an object. That object should have at least a `description` key. There is special handling for the following sub-keys:

- `group`: all inputs sharing the same `String` value will be rendered together in a dedicated table
    - required inputs are _always_ rendered under the "Required Inputs" table and thus should _not_ have a `group` key (it will be ignored if present)
    - The `Common` group of inputs will always come after the required inputs
    - Inputs without a `group` will be rendered under "Other Inputs" which will be the last input table
    - The `Resource` group of inputs will immediately precede the "Other Inputs" table
    - All other groups will render alphabetically between the `Common` table and the `Resource`table.
- `help`: will render at the top of the "Additional Meta" cell of the table

If an input has a `String` value for its parameter meta entry instead of a meta object, that string value will be treated as if it were the `description` key of a meta object.

### Outputs

Outputs should be documented in one of two places: either in the task/workflow `meta` section under an `outputs` key or at the root of the `parameter_meta` section.

Similar to inputs, each output should either be documented with an object which has a `description` key with a `String` value, or a `String` value directly. 

## Meta entries with special handling

Every struct, task, and workflow `meta` section should have a `description` key with a `String` value. This description string can have Markdown formatting. The `description` string should be less than 140 characters or it will be clipped in some contexts.

The `help` key should only have a `String` value, but can be of any length. It's best practice to keep `description` short, and put any additional text needed under the `help` key. Help strings can also be styled with Markdown!

The `external_help` key should have a URL as its value, and will be rendered as a button which will open a new tab or window visiting the link.

If the `warning` key has a `String` value, it will be rendered in a special "warning box" to draw the attention of users.

Workflows can have a `category` key which will group workflow pages on the left sidebar.
