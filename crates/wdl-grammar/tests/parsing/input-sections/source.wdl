# This is a test of input sections in tasks and workflows.

version 1.1

task t {
    input {
        String a
        Int b = 1 + 2
        String c = "Hello, ~{a}"
        Map[String, Int] d
    }
}

workflow w {
    input {
        String? a
        Int? b = 1 + 2
        String c = "Hello, ~{a}"
        Map[String, Int] d
        File e
        File f = "URL"
    }
}
