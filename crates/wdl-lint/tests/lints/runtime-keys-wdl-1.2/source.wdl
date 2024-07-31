#@ except: BlankLinesBetweenElements, CommentWhitespace, DescriptionMissing

version 1.2

task a_task_with_no_keys {
    meta {}
    command <<<>>>
    output {}
    runtime {} # This should not throw any errors for the runtime keys rule,
               # as the `runtime` section was deprecated in WDL v1.2.
}
