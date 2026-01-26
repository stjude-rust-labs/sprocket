#@ except: MetaDescription, ContainerUri

version 1.1

task a_task_with_no_keys {
    meta {}

    command <<<>>>

    output {}

    runtime {}  # Missing `container` key
}

task a_task_with_no_keys_but_they_are_excepted {
    meta {}

    command <<<>>>

    output {}

    #@ except: ExpectedRuntimeKeys
    runtime {}  # No errors should show.
}

task a_task_with_all_keys {
    meta {}

    command <<<>>>

    output {}

    runtime {
        container: "ubuntu"
        cpu: 1
        memory: "2 GiB"
        gpu: false
        disks: "1 GiB"
        maxRetries: 0
        returnCodes: 0
    }
}

task a_task_with_one_extra_key {
    meta {}

    command <<<>>>

    output {}

    runtime {
        container: "ubuntu"
        cpu: 1
        memory: "2 GiB"
        gpu: false
        disks: "1 GiB"
        maxRetries: 0
        returnCodes: 0
        foo: "bar"
    }
}

task a_task_with_two_extra_keys {
    meta {}

    command <<<>>>

    output {}

    runtime {
        container: "ubuntu"
        cpu: 1
        memory: "2 GiB"
        gpu: false
        disks: "1 GiB"
        maxRetries: 0
        returnCodes: 0
        foo: "bar"
        baz: "quux"
    }
}

# https://github.com/stjude-rust-labs/sprocket/issues/538
task a_task_with_an_allowed_key {
    meta {}

    command <<<>>>

    output {}

    runtime {
        container: "ubuntu"
        cpu: 1
        memory: "2 GiB"
        gpu: false
        disks: "1 GiB"
        maxRetries: 0
        returnCodes: 0
        some_allowed_key: "bar"
    }
}

task a_task_with_an_explicitly_excepted_key {
    meta {}

    command <<<>>>

    output {}

    runtime {
        container: "ubuntu"
        cpu: 1
        memory: "2 GiB"
        gpu: false
        disks: "1 GiB"
        maxRetries: 0
        returnCodes: 0
        #@ except: ExpectedRuntimeKeys
        this_key_is_allowed: "bar"
        this_key_is_not_allowed: "baz"
    }
}
