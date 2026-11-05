version 1.4

task messy {
    input {
        String name
        Int age = 0
        validation {length(name) > 0:"name must not be empty",age>=0  :   "age must not be negative"}
    }

    command <<<>>>

    output {
        String label = name



        # comment before the validation block
        validation
        {
            # comment before a clause
            length(label) > 1 : "label must not be empty" ,  # inline comment
            label != name: "label must differ"   # no trailing comma
            ,
        }
    }
}

workflow messy_workflow {
    input {
        String name
        validation { true: "a", }
    }

    output {
        String out = name
        validation {
        }
    }
}

task empty {
    input { validation {} }
    command <<<>>>
}
