# Verify that `ExceptDirectiveValid` works with `wdl-lint` rules
#@ except: EmptyOutputs, BashSetSyntax, RequirementsSection, MetaSections
version 1.3

#@ except: ConciseInput
task foo {
    command <<<>>>
}