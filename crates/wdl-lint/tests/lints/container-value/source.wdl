#@ except: DescriptionMissing, Todo, MissingRequirements

## This is a test of the `ContainerValue` lint.

version 1.2

task a {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should be flagged for a missing tag.
        container: "ubuntu"
    }
}

task b {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should be flagged for a mutable tag.
        container: "ubuntu:latest"
    }
}

task c {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should NOT be flagged, because a tag exists and it is immutable.
        container: "ubuntu@sha256:ThisRepresentsASHASum"
    }
}

task d {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should NOT be flagged because the tag is malformed (and, thus, we cannot
        # lint it). Eventually, this will be handled by validation (TODO).
        container: "ubuntu@sha256:"
    }
}

task e {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    runtime {
        # This is the same as `task a` but with the deprecated 'docker' key name, so it
        # should be flagged as missing a tag.
        docker: "ubuntu"
    }
}

task f {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    runtime {
        # This is the same as `task b` but with the deprecated 'docker' key name, so it
        # should be flagged as a mutable tag.
        docker: "ubuntu:latest"
    }
}

task g {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should NOT be flagged.
        container: "*"
    }
}

task h {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should flagged indicating that the element should be a single string.
        container: ["*"]
    }
}

task i {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should flagged as an array containing anys
        container: ["*", "foo", "*", "*"]
    }
}

task j {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should flagged as an empty array.
        container: []
    }
}

task k {
    meta {}

    parameter_meta {}

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should flagged as missing the `container` key.
    }
}

task l {
    meta {}

    parameter_meta {
        image: "The docker image to use"
    }

    input {
        String image
    }

    command <<<
        echo "Hello, World!"
    >>>

    output {}

    requirements {
        # This should not fire anything, as we don't currently parse strings
        # with placeholders.
        #
        # TODO(clay): perhaps we can parse out just the tags in this particular
        # case where the placeholder gives the container image name?
        container: "${image}:latest"
    }
}
