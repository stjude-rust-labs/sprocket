version 1.2

struct Person {
    String name
    File? cv
}

task input_hint {
    input {
        Person person
    }

    command <<<
    echo "Hello, ~{person.name}!"
    >>>

    requirements {
        container: "ubuntu:latest"
    }

    output {
        String greeting = read_string(stdout())
    }

    hints {
        inputs: input {
            person.name: hints {
                min_length: 3
            },
            person.cv: hints {
                localization_optional: true
            }
        }
        outputs: output {
            greeting: hints {
                max_length: 100
            }
        }
    }
}
