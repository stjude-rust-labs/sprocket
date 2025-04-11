#@ except: InputSorted, InputName, OutputName

version 1.1

import "baz"  # following whitespace will be caught by ImportWhitespace rule

import "qux"  # following whitespace duplication is caught be Whitespace rule


# test comment
workflow foo {

    # This is OK (but the prior line is not).
    #@ except: MetaDescription
    meta {
    }
    # above is ok but the next won't be
    parameter_meta {

    }
    # what about this comment?
    input {

    }
    scatter (i in ["hello", "world"]) {
        call bar { input: s = i }

    }
    if (true) {
        call bar { input: s = "world" }

    }
    String p = "pip"


    String q = "bar"  # The following whitespace is allowable between private declarations

    String r = "world"
    String s = "hello"  # following whitespace duplication is caught be Whitespace rule


    call bar { input:
        s

    }  # following whitespace duplication is caught be Whitespace rule


    call bar as baz { input:
        s
    }
    call bar as qux { input:  # Calls may optionally be separated by whitespace.
        s
    }

    output {

    }
}
#@ except: MetaSections, RuntimeSection
task bar {

    meta {

        description: "bar"

        outputs: {
            u: "u"

        }

    }

    input {
        String s = "hello"

        String? t
    }

    command <<< >>>

    output {
        String u = "u"
    }

}

task bax {
    #@ except: MetaDescription
    meta {}

    parameter_meta {}


    input {}

    command <<< >>>

    output {}

    #@ except: ContainerUri
    runtime {

        disks: "50 GB"
        memory: "4 GB"

        container: "ubuntu:latest"

    }
}
