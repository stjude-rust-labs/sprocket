# This is a test of runtime sections in tasks.

version 1.1

task test {
    runtime {
        container: "ubuntu:latest"
        maxMemory: "36 GB"
        maxCpu: 24
        shortTask: true
        localizationOptional: false
        inputs: object {
            foo: object { 
                localizationOptional: true
            }
        }
    }
}
