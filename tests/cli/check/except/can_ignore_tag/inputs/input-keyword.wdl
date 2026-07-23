# Test for lint tag exceptions. This file should only produce rules under the
# `Deprecated` tag, which is excepted.
#@ except: EmptyOutputs, MetaSections

version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        set -euo pipefail
        echo "Hello, ~{name}!"
    >>>
}

workflow test {
    call say_hello { input:
        name = "World"
    }
}