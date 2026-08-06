version 1.4

import { grind_beans } from coffeeshop

task grind_beans {
    command <<<
        echo "local grind"
    >>>

    output {
        String ground_coffee = read_string(stdout())
    }
}

workflow make_coffee {
    call grind_beans

    output {
        String result = grind_beans.ground_coffee
    }
}
