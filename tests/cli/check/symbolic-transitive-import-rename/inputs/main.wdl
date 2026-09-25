version 1.4

import { make_espresso as brew_shot } from cafe_menu

workflow make_coffee {
    call brew_shot {
        input:
            ground_coffee = "dark roast grounds"
    }

    output {
        String result = brew_shot.espresso
    }
}
