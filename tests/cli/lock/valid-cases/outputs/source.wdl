version 1.3

task docker_good {
    command <<<>>>

    requirements {
        container: "quay.io/lib/ubuntu:latest"
    }
}

task docker_good2 {
    command <<<>>>

    requirements {
        container: ["quay.io/lib/debian:bookworm", "quay.io/lib/debian:trixie"]
    }
}

task oras_good {
    command <<<>>>

    requirements {
        container: "oras://ghcr.io/stjude-rust-labs/sprocket:v0.23.0"
    }
}
