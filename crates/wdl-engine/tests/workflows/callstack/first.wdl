version 1.3

task my_task {
    input {
        String x
        Int y
        Float z
    }

    command <<<
        >&2 printf "first! (should not be present)\nsecond!\nthird!\nfourth!\nfifth!\nsixth!\nseventh!\neighth!\nninth!\ntenth!\neleventh!\n"
        exit 1
    >>>
}

workflow test {
    call my_task {
        x = "foo",
        y = 0,
        z = 1.0
    }
}
