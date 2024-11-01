version 1.0

workflow test_wf {
    meta {
        a: "hello"
        b: "world"
        c: 5
        d: -0xf
        e: 1.0e10
        f: -2.
        g: true
        h: false
        i: null
        j: {
            a: [
                1,
                2,
                3,
            ],
            b: [
                "hello",
                "world",
                "!",
            ],
            c: {
                x: 1,
                y: 2,
                z: 3,
            },
        }
        k: [
            {
                a: {},
                b: 0,
                c: "",
                d: "",
                e: [],
            },
            {
                x: [
                    1.0,
                    2.0,
                    3.0,
                ],
            },
        ]
    }

    parameter_meta {
        out_sj_filter_overhang_min: {
            type: "SpliceJunctionMotifs",
            label: "Minimum overhang required to support a splicing junction",
        }
    }

    input {
        SpliceJunctionMotifs out_sj_filter_overhang_min = SpliceJunctionMotifs {
            noncanonical_motifs: 30,
            GT_AG_and_CT_AC_motif: 12,
        }
    }

    call no_params call with_params { input:
        a,
        b,
        c,
        d = 1,
    }
    call qualified.name call qualified.name { input:
        a = 1,
        b = 2,
        c = "3",
    }
    call aliased as x call aliased as x { input:
    }
    call f after x after y call f after x after y { input: a = [] }
    call f as x after x call f as x after x after y { input: name = "hello" }
    call test_task as foo { input: bowchicka = "wowwow" }
    if (true) {

        call test_task after foo { input: bowchicka = "bowchicka" }
        scatter (i in range(3)) {
            call test_task as bar { input: bowchicka = i * 42 }
        }
    }

    output {
        SpliceJunctionMotifs KAZAM = out_sj_filter_overhang_min
        String a = "friend"
        Int b = 1 + 2
        String c = "Hello, ~{a}"
        Map[String, Int] d = {
            "a": 0,
            "b": 1,
            "c": 2,
        }
    }
}

task test_task {
    parameter_meta {
        bowchicka: {
            type: "String",
            label: "Bowchicka",
        }
    }

    input {
        String bowchicka
    }

    command <<<
    >>>
}

struct SpliceJunctionMotifs {
    Int noncanonical_motifs
    Int GT_AG_and_CT_AC_motif
}
