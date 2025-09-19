## This is a test of type checking `runtime` keys. 

version 1.1

task foo {
    command <<<>>>

    runtime {
        container: "foo/bar"
        cpu: 1
        memory: 1
        gpu: true
        fpga: false
        disks: 1
        max_retries: 0
        return_codes: 0
        unsupported: false
    }
}

task bar {
    command <<<>>>

    runtime {
        container: ["foo/bar", "bar/baz"]
        cpu: 1.0
        memory: "1GiB"
        gpu: false
        fpga: true
        disks: "1GiB"
        maxRetries: 1
        return_codes: "0"
        unsupported: false
    }
}

task baz {
    command <<<>>>

    runtime {
        docker: "foo/bar"
        disks: ["1GiB", "2GiB"]
        return_codes: [1, 2, 3]
        unsupported: false
    }
}

task jam {
    command <<<>>>

    runtime {
        docker: ["foo/bar"]
        returnCodes: 1
        unsupported: false
    }
}

task incorrect {
    command <<<>>>
    
    runtime {
        container: false
        cpu: false
        memory: false
        gpu: "false"
        fpga: "false"
        disks: false
        max_retries: false
        return_codes: false
    }
}
