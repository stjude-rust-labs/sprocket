## Enchantment primitives for raw magical runes.
version 1.4

## Summary of a completed enchantment.
struct EnchantmentSummary {
    ## Bound tome file.
    File tome

    ## Fraction of runes bound into the tome.
    Float bound_fraction

    ## Number of duplicate runes.
    Int duplicate_runes
}

## Binds raw runes into a tome against a named spellbook.
task bind_runes {
    input {
        ## Runes to bind.
        File runes

        ## Spellbook identifier.
        String spellbook

        ## Optional sigil identifier.
        String? sigil
    }

    command <<<
        printf '%s %s %s\n' '~{runes}' '~{spellbook}' '~{default="none" sigil}' > bind-inputs.txt
        touch bound.tome
        printf '0.97\n' > bound_fraction.txt
        printf '1200\n' > duplicate_runes.txt
    >>>

    output {
        ## Bound tome.
        File tome = "bound.tome"

        ## Fraction of runes bound into the tome.
        Float bound_fraction = read_float("bound_fraction.txt")

        ## Number of runes marked as duplicates.
        Int duplicate_runes = read_int("duplicate_runes.txt")
    }

    requirements {
        container: "ubuntu:latest"
        cpu: 4
        memory: "8 GiB"
    }
}
