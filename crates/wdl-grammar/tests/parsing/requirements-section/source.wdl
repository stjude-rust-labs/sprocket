## This is a test of parsing a requirements section.

version 1.2

task test {
    requirements {
        container: "ubuntu:latest"
        memory: "2 GiB"
        gpu: true
        disks: ["2", "/mnt/outputs 4 GiB", "/mnt/tmp 1 GiB"]
        max_retries: 4
        return_codes: 1
    }
}
