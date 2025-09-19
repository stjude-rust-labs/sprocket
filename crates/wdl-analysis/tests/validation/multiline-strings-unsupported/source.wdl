## This is a test of detecting unsupported multi-line strings.

version 1.1

task foo {
    command <<<>>>
}

workflow test {
    meta {
        foo: <<< not supported! >>>
    }

    String x = <<< not supported! >>>
}
