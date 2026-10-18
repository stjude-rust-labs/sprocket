version 1.4

import { pull_espresso } from coffeeshop

workflow daily_special {
    input {
        String ground_coffee
    }

    call pull_espresso {
            ground_coffee = ground_coffee
    }

    output {
        String espresso = pull_espresso.espresso
    }
}
