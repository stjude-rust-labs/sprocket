## Test cases for placeholder options analysis
#@ except: UnusedDeclaration

version 1.2

task valid_case {
    input {
        Array[String] arr
    }
    command <<< ~{sep=", " arr} >>> # OK: sep option with an array
}

task invalid_case {
    command <<< ~{sep=", " true} >>> # NOT OK: sep expects an array, but got Boolean
}

workflow placeholder_options_test {
    String? var = None

    # OK: Boolean is coercible to string, no option
    String s1 = "~{true}"

    # OK: true/false option with Boolean
    String s2 = "~{true="yes" false="no" false}"

    # NOT OK: true/false expects Boolean, but got Int
    String s3 = "~{true="yes" false="no" 1}"

    # OK: sep option with Array[Int]
    String s4 = "~{sep=',' [1, 2, 3]}"

    # NOT OK: sep expects Array, but got Int
    String s5 = "~{sep=',' 123}"

    # OK: default option with optional variable
    String s6 = "~{default="fallback" var}"

    # NOT OK: default expects primitive, but got Array
    String s7 = "~{default="fallback" [123]}"

    # NOT OK: Array without sep option, not coercible to string
    String s8 = "~{[1, 2, 3]}"

    # OK: Union with or without options
    String s9 = "~{sep=',' read_json('foo.json')}"
    String s10 = "~{default='nope' read_json('foo.json')}"
    String s11 = "~{true='y' false='n' read_json('foo.json')}"
    String s12 = "~{read_json('foo.json')}"
}
