version 1.4

import { grind_beans, steam_milk } from coffeeshop

workflow make_coffee {
    call grind_beans {
            roast_level = "dark"
    }

    call steam_milk {
            milk_type = "oat"
    }

    output {
        String ground = grind_beans.ground_coffee
        String milk = steam_milk.steamed_milk
    }
}
