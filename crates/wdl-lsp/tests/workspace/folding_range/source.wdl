version 1.3

import "foo.wdl"
import "bar.wdl"

# I'm a plain line comment.
# Lorem ipsum dolor sit amet consectetur adipiscing elit.
# Quisque faucibus ex sapien vitae pellentesque sem placerat.
# In id cursus mi pretium tellus duis convallis.
# All part of a single block.

# I'm a separate line comment.
# Different folding range for me!

## I'm a collapsible doc comment.
## And I'm part of the same block.
# Interrupting the block
## Starting a new block
## Keep going...
task baz {
    meta {
        description: "The meta section is totally collapsible"
    }

    parameter_meta {
        unused: "So is the parameter meta"
    }

    command <<<
        echo "Commands too"
    >>>
}
