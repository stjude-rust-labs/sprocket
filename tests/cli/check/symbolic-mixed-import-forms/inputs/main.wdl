version 1.4

import coffeeshop
import { pour_latte_art } from cafe_menu

workflow make_latte {
    call coffeeshop.grind_beans {
            roast_level = "medium"
    }

    call coffeeshop.pull_espresso {
            ground_coffee = grind_beans.ground_coffee
    }

    call coffeeshop.steam_milk {
            milk_type = "oat"
    }

    call pour_latte_art {
            espresso = pull_espresso.espresso,
            steamed_milk = steam_milk.steamed_milk
    }

    output {
        String latte = pour_latte_art.latte
    }
}
