version 1.4

import coffeeshop

workflow make_coffee {
    call coffeeshop.grind_beans {
        input:
            roast_level = "dark"
    }

    output {
        String result = grind_beans.ground_coffee
    }
}
