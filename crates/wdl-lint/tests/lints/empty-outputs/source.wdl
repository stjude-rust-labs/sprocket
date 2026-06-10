#@ except: MetaSections, OutputName, RequirementsSection

version 1.2

task no_outputs {
    command <<<
        touch foo.txt
    >>>
}

#@ except: EmptyOutputs
task no_outputs_excepted {
    command <<<
        touch foo.txt
    >>>
}

task empty_outputs {
    command <<<
        touch foo.txt
    >>>

    output {
    }
}

task outputs {
    command <<<
        touch foo.txt
    >>>

    output {
        File outputs = "foo.txt"
    }
}
