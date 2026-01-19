## This is a test of type checking `requirements` keys. 

version 1.3

task foo {
    command <<<>>>

    requirements {
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

    requirements {
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

    requirements {
        docker: "foo/bar"
        disks: ["1GiB", "2GiB"]
        return_codes: [1, 2, 3]
        unsupported: false
    }
}

task jam {
    command <<<>>>

    requirements {
        docker: ["foo/bar"]
        returnCodes: 1
        unsupported: false
    }
}

task incorrect {
    command <<<>>>
    
    requirements {
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
