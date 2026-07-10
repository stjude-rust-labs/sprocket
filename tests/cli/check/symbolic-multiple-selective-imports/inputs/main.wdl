version 1.4

import { grind_beans, steam_milk } from coffeeshop

workflow make_coffee {
    call grind_beans {
        input:
            roast_level = "dark"
    }

    call steam_milk {
        input:
            milk_type = "oat"
    }

    output {
        String ground = grind_beans.ground_coffee
        String milk = steam_milk.steamed_milk
    }
}
