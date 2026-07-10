version 1.4

import { pour_latte_art } from coffeeshop

workflow latte_special {
    input {
        String espresso
        String steamed_milk
    }

    call pour_latte_art {
        input:
            espresso = espresso,
            steamed_milk = steamed_milk
    }

    output {
        String latte = pour_latte_art.latte
    }
}
