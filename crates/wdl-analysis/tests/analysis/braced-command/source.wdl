## This is a test of analyzing a braced command.

version 1.1

task test {
    command {
        echo ${x}
        echo ~{unknown}
    }

    String x = "hi"
}
