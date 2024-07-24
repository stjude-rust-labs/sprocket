#@ except: CommentWhitespace, DescriptionMissing

version 1.1

task a_task_with_multiple_runtimes {
    meta {}
    command <<<>>>
    output {}

    # The lints should only appear for this runtime.
    runtime {
        foo: "bar" # these items should be processed and flagged.
        baz: "quux"
    }

    # This should be reported as a validation error with no
    # lint warnings.
    runtime {
        foo: "bar" # these items should not be processed and flagged.
        baz: "quux"
    }
}
