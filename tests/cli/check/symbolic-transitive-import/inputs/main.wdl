version 1.4

import { pull_espresso } from cafe_menu

workflow make_coffee {
    call pull_espresso {
        input:
            ground_coffee = "dark roast grounds"
    }

    output {
        String result = pull_espresso.espresso
    }
}
