#@ except: DescriptionMissing, ContainerValue

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

    #@ except: RuntimeSectionKeys
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
