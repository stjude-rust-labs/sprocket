## This is a test of type checking `runtime` keys in a WDL 1.0 document.
##
## WDL 1.0 does not formally define the `cpu`, `gpu`, `disks`, `maxRetries`,
## or `returnCodes` runtime keys, so no type checking should be performed on
## them here; only `docker` and `memory` have recommended type conventions in
## 1.0 and should still be type checked.

version 1.0

task foo {
    command <<<>>>

    runtime {
        docker: "foo/bar"
        memory: "1GiB"
        cpu: "1"
        gpu: "1"
        disks: 1
        maxRetries: "1"
        returnCodes: "*"
        unsupported: false
    }
}

task incorrect {
    command <<<>>>

    runtime {
        docker: false
        memory: false
        cpu: false
        gpu: false
        disks: false
        maxRetries: false
        returnCodes: false
    }
}