version 1.4

import coffeeshop
import { pour_latte_art } from cafe_menu

workflow make_latte {
    call coffeeshop.grind_beans {
        input:
            roast_level = "medium"
    }

    call coffeeshop.pull_espresso {
        input:
            ground_coffee = grind_beans.ground_coffee
    }

    call coffeeshop.steam_milk {
        input:
            milk_type = "oat"
    }

    call pour_latte_art {
        input:
            espresso = pull_espresso.espresso,
            steamed_milk = steam_milk.steamed_milk
    }

    output {
        String latte = pour_latte_art.latte
    }
}
