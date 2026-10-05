#@ except: BashSetSyntax, ContainerUri, MutableContainerTag, RedundantContainerArray, EmptyOutputs, MetaSections

version 1.3

# TODO: this is no longer flagged since `TODO` is not in `keywords`.
# FIXME: this is flagged.
task foo {
    command <<<>>>

    output {}

    requirements {
        container: "ubuntu:latest"
    }
}
