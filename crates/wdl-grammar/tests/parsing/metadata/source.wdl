# This is a test of parsing task and workflow metadata sections.

version 1.1

task test {
    meta {
        a: "hello"
        b: 'world'
        c: 5
        d: -0xf
        e: 1.0e10
        f: -2.
        g: true
        h: false
        i: null
        j: {
            a: [1, 2, 3],
            b: ["hello", "world", "!"],
            c: {
                x: 1,
                y: 2,
                z: 3
            }
        }
        k: [
            {
                a: {},
                b: 0,
                c: "",
                d: '',
                e: [],
            },
            {
                x: [1.0, 2.0, 3.0]
            }
        ]
    }
    
    parameter_meta {
        a: "hello"
        b: 'world'
        c: 5
        d: -0xf
        e: 1.0e10
        f: -2.
        g: true
        h: false
        i: null
        j: {
            a: [1, 2, 3],
            b: ["hello", "world", "!"],
            c: {
                x: 1,
                y: 2,
                z: 3
            }
        }
        k: [
            {
                a: {},
                b: 0,
                c: "",
                d: '',
                e: [],
            },
            {
                x: [1.0, 2.0, 3.0]
            }
        ]
    }
}

workflow w {
    meta {
        a: "hello"
        b: 'world'
        c: 5
        d: -0xf
        e: 1.0e10
        f: -2.
        g: true
        h: false
        i: null
        j: {
            a: [1, 2, 3],
            b: ["hello", "world", "!"],
            c: {
                x: 1,
                y: 2,
                z: 3
            }
        }
        k: [
            {
                a: {},
                b: 0,
                c: "",
                d: '',
                e: [],
            },
            {
                x: [1.0, 2.0, 3.0]
            }
        ]
    }
    
    parameter_meta {
        a: "hello"
        b: 'world'
        c: 5
        d: -0xf
        e: 1.0e10
        f: -2.
        g: true
        h: false
        i: null
        j: {
            a: [1, 2, 3],
            b: ["hello", "world", "!"],
            c: {
                x: 1,
                y: 2,
                z: 3
            }
        }
        k: [
            {
                a: {},
                b: 0,
                c: "",
                d: '',
                e: [],
            },
            {
                x: [1.0, 2.0, 3.0]
            }
        ]
    }
}
