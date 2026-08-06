version 1.4

import { grind_beans as prep_beans } from coffeeshop

workflow make_coffee {
    call prep_beans {
        input:
            roast_level = "dark"
    }

    output {
        String result = prep_beans.ground_coffee
    }
}
