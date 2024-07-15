#@ except: DescriptionMissing

task a_task_with_engine_hints {
    meta {}
    command <<<>>>
    output {}
    runtime {
        container: "ubuntu"
        cpu: 1
        disks: []
        gpu: false
        maxRetries: 1
        memory: "1 GiB"
        returnCodes: "*"
        cromwell: {}
        miniwdl: {}
    }
}

task a_task_with_excepted_engine_hints {
    meta {}
    command <<<>>>
    output {}
    runtime {
        container: "ubuntu"
        cpu: 1
        disks: []
        gpu: false
        maxRetries: 1
        memory: "1 GiB"
        returnCodes: "*"
        #@ except: RuntimeSectionKeys
        cromwell: {}
        #@ except: RuntimeSectionKeys
        miniwdl: {}
    }
}
