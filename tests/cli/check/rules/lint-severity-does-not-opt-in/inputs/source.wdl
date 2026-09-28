#@ except: BashSetSyntax, ContainerUri, MutableContainerTag, RedundantContainerArray, EmptyOutputs, MetaSections

version 1.3

# TODO: this comment is only flagged when the `Style` tag is enabled.
task foo {
    command <<<>>>

    output {}

    requirements {
        container: "ubuntu:latest"
    }
}
