## This is a test of the `DenyGlobStar` lint.
#@ except: DoubleQuotes, ExpectedRuntimeKeys, MetaDescription
version 1.3

task bad {
    meta {
        description: "This task should be flagged for using a glob with a star in the output section."
        outputs: {
            everything: "not OK",
            everything2: "not OK",
        }
    }

    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should be flagged for using a glob with a star.
        Array[File] everything = glob("*")
        # This should be flagged for using a glob with a star.
        Array[File] everything2 = glob('*')
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

task good {
    meta {
        description: "This task should not be flagged, because the glob is used correctly, or is excepted."
        outputs: {
            everything: "OK",
            everything2: "EXCEPTED",
        }
    }

    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should not be flagged, because the glob contains a star but are used intentionally.
        Array[File] everything = glob("*.vcf")
        #@ except: DenyGlobStar
        Array[File] everything2 = glob("*")
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

task good_excepted {
    meta {
        description: "This task should not be flagged, because the glob contains a star but is used intentionally."
        outputs: {
            everything: "EXCEPTED",
            everything2: "EXCEPTED",
        }
    }

    command <<<
        echo "Hello, World!"
    >>>

    #@ except: DenyGlobStar
    output {
        Array[File] everything = glob("*")
        Array[File] everything2 = glob("*")
    }

    requirements {
        container: "ubuntu@sha256:foobar"
    }
}
