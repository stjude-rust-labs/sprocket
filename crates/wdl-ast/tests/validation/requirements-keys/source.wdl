## This is a test of unsupported requirements keys.

version 1.2

task foo {
    command <<<>>>

    requirements {
        container: "foo/bar"
        cpu: 2
        memory: "1 GiB"
        gpu: true
        fpga: true
        unsupported_key: true
        disks: 10
        max_retries: 1
        return_codes: 1
    }
}

task bar {
    command <<<>>>

    requirements {
        docker: "foo/bar"
        maxRetries: 1
        unsupported_key: true
        returnCodes: 1
    }
}

task baz {
    command <<<>>>

    # Check for conflicting keys
    requirements {
        container: "foo/bar"
        docker: "foo/bar"
        max_retries: 1
        maxRetries: 1
        return_codes: 1
        returnCodes: 1
    }
}
