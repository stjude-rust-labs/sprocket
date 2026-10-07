version 1.3

task copy {
    input {
        File to_copy
    }
    command <<<
        cp ~{to_copy} out.txt
    >>>
    output {
        File copied = "out.txt"
    }
}

# Three workflows deep
workflow copy_wf {
    input {}

    call copy { }
    output {
        File copied = copy.copied
    }

    hints {
        allow_nested_inputs: true
    }
}
