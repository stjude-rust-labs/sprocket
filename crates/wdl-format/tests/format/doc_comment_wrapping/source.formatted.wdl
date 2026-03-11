## This is a very very very very very very long paragraph that should be wrapped correctly
## by the markdown formatter when it exceeds the maximum line length
version 1.2

## Short description that fits on one line
##
## This is a longer paragraph that contains enough text to require wrapping. The formatter
## should break this into multiple lines while respecting the indentation level and the
## doc comment prefix width.
##
## - First bullet point that is short
## - Second bullet point that is intentionally written to be very long so that the markdown
##   formatter will wrap it to the next line with proper continuation indentation
## - Third bullet point
##
## ```
## code_block_should_not_be_modified()
## even_if_lines_are_very_very_very_very_very_very_very_very_very_very_very_very_very_long
## ```
##
## Another paragraph after the code block.
task my_task {
    input {
        ## This input parameter has a very long doc comment description that explains what the
        ## parameter does and how to use it properly in the workflow.
        String long_param

        ## Short doc.
        Int x
    }

    command <<<
        echo "hello"
    >>>

    output {
        ## The output file. This is a very long doc comment for the output that should also be
        ## wrapped appropriately by the formatter.
        File result = "output.txt"
    }
}

## Workflow-level doc comment that is short
workflow my_workflow {
    ## This is a deeply indented doc comment inside a workflow body that should still respect
    ## indentation when wrapping long text in the markdown formatter.
    input {
        ## A parameter
        String name
    }
}
