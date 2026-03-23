## This is a test of the `DenyGlobStar` lint.
#@ except: MetaDescription, ExpectedRuntimeKeys


version 1.3

task bad {
    meta {
        description: "This task should be flagged for using a glob with a star in the output section."
        outputs: {
            everything: "not OK"
        }
    }

    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should be flagged for using a glob with a star.
        Array[File] everything = glob("*")
    }
    requirements {
        container: "ubuntu@sha256:foobar"
    }

   
}

task good_no_star {
    meta {
        description: "This task should not be flagged, because the glob does not contain a star or is excepted."
        outputs: {
            everything: "OK",
            everything2: "EXCEPTED",
        }
    }

    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should not be flagged, because the glob does not contain a star.
        Array[File] everything = glob("test")
        #@ except: DenyGlobStar
        Array[File] everything2= glob("*")
    }
    requirements {
        container: "ubuntu@sha256:foobar"
    }

   
}

task good_star {
    meta {
        description: "This task should not be flagged, because the glob contains a star but is used intentionally."
        outputs: {
            everything: "OK"
        }
    }
    
    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should not be flagged, because the glob contains a star but are used intentionally.
        Array[File] everything = glob("*.vcf")
    }
    requirements {
        container: "ubuntu@sha256:foobar"
    }

}

task good_star_multiline {
    meta {
        description: "This task should not be flagged, because the glob contains a star but is used intentionally."
        outputs: {
            everything: "OK",
            everything2: "OK",
            everything3: "OK",
        }
    }
    
    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should not be flagged, because the glob contains a star but are used intentionally.
        Array[File] everything = glob("*.vcf")
        String? everything2 = find("*", "123456*")
        Array[File] everything3 = glob("*.bed")

    }
    requirements {
        container: "ubuntu@sha256:foobar"
    }
}
task good_star_excepted {
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

task bad_star_multiline {
    meta {
        description: "This task should be flagged, because the glob contains a star and is not used intentionally."
        outputs: {
            everything: "OK",
            everything2: "NOT OK",
            everything3: "OK",
            everything4: "OK",
        }
    }
    
    command <<<
        echo "Hello, World!"
    >>>

    output {
        # This should be flagged, because everything2 glob contains a star.
        Array[File] everything = glob("*.vcf")
        Array[File] everything2 = glob("*")
        Int everything3 = length("*")
        Array[File] everything4 = glob("*.test")
    }
    requirements {
        container: "ubuntu@sha256:foobar"
    }
}

