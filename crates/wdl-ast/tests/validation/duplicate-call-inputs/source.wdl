## This is a test for duplicate call inputs.

version 1.1

workflow wf {
    call a {
        input:
            x = 1,
            x = 2
    }
}
