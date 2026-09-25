version 1.0

task grind_beans {
    input {
        String roast_level
    }

    command <<<
        echo "grinding ~{roast_level} beans"
    >>>

    output {
        String ground_coffee = read_string(stdout())
    }
}

task steam_milk {
    input {
        String milk_type
    }

    command <<<
        echo "steaming ~{milk_type} milk"
    >>>

    output {
        String steamed_milk = read_string(stdout())
    }
}

task pull_espresso {
    input {
        String ground_coffee
    }

    command <<<
        echo "pulling espresso from ~{ground_coffee}"
    >>>

    output {
        String espresso = read_string(stdout())
    }
}

task pour_latte_art {
    input {
        String espresso
        String steamed_milk
    }

    command <<<
        echo "pouring latte art with ~{espresso} and ~{steamed_milk}"
    >>>

    output {
        String latte = read_string(stdout())
    }
}
