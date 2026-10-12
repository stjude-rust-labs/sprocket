version 1.2

task t {
    input {
        String bar
    }

    command <<<
        echo ~{
            #@ except: ShellSplitting
            bar
        }
    >>>

    output {
    }
}
