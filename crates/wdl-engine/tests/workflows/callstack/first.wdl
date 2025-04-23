version 1.2

task my_task {
    command <<<
        >&2 printf "first! (should not be present)\nsecond!\nthird!\nfourth!\nfifth!\nsixth!\nseventh!\neighth!\nninth!\ntenth!\neleventh!\n"
        exit 1
    >>>
}

workflow test {
    call my_task
}
