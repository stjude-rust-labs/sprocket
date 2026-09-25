# https://github.com/stjude-rust-labs/sprocket/pull/1060
#
# Rules excepted over the CLI should still be known to the validator.
#@ except: EmptyOutputs, BashSetSyntax, MetaSections, RequirementsSection

version 1.3

task foo {
    #@ except: ShellCheck
    command <<<
        echo "Hello, world!"
    >>>
}
