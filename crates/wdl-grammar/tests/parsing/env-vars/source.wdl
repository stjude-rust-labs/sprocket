version 1.2

task foo {
    input {
        String a
        env String b
    }

    env String c = ""

    output {
        env String d = ""
    }
}

workflow bar {
    input {
        String e
        env String f
    }

    env String g = ""

    output {
        env String h = ""
    }
}