version development

task hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    output {
        String greeting = read_string(stdout())
    }
}

workflow test {
    input {
        String test_name
    }

    call hello {
        input:
            name = test_name
    }

    output {
        String result = hello.greeting
    }
}