version 1.2

workflow test {
    call task_a {
        input:
            a = 1,
            b = 2,
    }

    call task_b {
        input: # comment on input
            c = 3,
    }

    call task_c {
        input:
            d = 4,
    }
}
