## End-to-end spellcraft workflows.
version 1.4

import "modules/wards/wards.wdl" as wards
import { EnchantmentSummary, bind_runes } from "modules/enchantment/enchantment.wdl"

## Supported spellbook editions.
enum Spellbook {
    Elder,
    Arcane
}

## A magical scroll and its enchantment metadata.
struct Scroll {
    ## Stable scroll identifier.
    String id

    ## Raw magical runes.
    File runes

    ## Spellbook used for enchantment.
    Spellbook spellbook

    ## Optional sigil identifier.
    String? sigil
}

## Wards and enchants one magical scroll.
##
## This workflow demonstrates namespace and selective imports, structured
## inputs, task calls, and typed outputs.
workflow enchant_scroll {
    input {
        ## Scroll to enchant.
        Scroll scroll

        ## Minimum rune count accepted by warding.
        Int minimum_runes = 100000
    }

    call wards.inspect_wards {
        input:
            runes = scroll.runes,
            minimum_runes = minimum_runes
    }

    call bind_runes {
        input:
            runes = scroll.runes,
            spellbook = if scroll.spellbook == Spellbook.Arcane then "Arcane" else "Elder",
            sigil = scroll.sigil
    }

    output {
        ## Bound tome file.
        File tome = bind_runes.tome

        ## Enchantment summary assembled from task outputs.
        EnchantmentSummary enchantment = EnchantmentSummary {
            tome: bind_runes.tome,
            bound_fraction: bind_runes.bound_fraction,
            duplicate_runes: bind_runes.duplicate_runes,
        }

        ## Total runes observed during warding.
        Int total_runes = inspect_wards.total_runes
    }

    meta {
        description: "Runs the standard warding and enchantment pipeline."
        outputs: {
            tome: "Bound tome of enchanted runes.",
            enchantment: "Typed summary of enchantment outputs.",
            total_runes: "Total input runes observed by warding.",
        }
    }

    parameter_meta {
        scroll: "Magical scroll and spellbook metadata."
        minimum_runes: "Minimum rune count required by warding."
    }
}
