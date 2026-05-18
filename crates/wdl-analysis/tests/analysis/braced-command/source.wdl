## This is a test of analyzing a braced command.

version 1.1

task test {
    String x = "hi"

    command {
        echo ${x}
        echo ~{unknown}
    }
}
