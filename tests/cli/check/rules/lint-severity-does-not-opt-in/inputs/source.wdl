#@ except: MetaSections

version 1.2

# TODO: this comment is only flagged when the `Style` tag is enabled.
task foo {
    command <<<>>>

    output {}

    requirements {
        container: "ubuntu:latest"
    }
}
