version 1.4

import { pull_espresso as make_espresso } from coffeeshop

workflow daily_special {
    input {
        String ground_coffee
    }

    call make_espresso {
        input:
            ground_coffee = ground_coffee
    }

    output {
        String espresso = make_espresso.espresso
    }
}
