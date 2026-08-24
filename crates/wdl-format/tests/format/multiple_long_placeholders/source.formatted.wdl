version 1.3

task cast_spell {
    command <<<
        --summonSpectralFamiliar ~{arcane_focus_gem.left} ~{(if (
            length(spell_components) != 0
        )
            then arcane_focus_gem.right
            else None)} \
        --wardOuterPerimeter ~{binding_sigil_ink.left} "~{(if (length(
            spell_components
        ) != 0)
            then binding_sigil_ink.right
            else None)}" \
        --brewElixir ~{mandrake_root.left} ~{(if (length(
            spell_components
        ) != 0)
            then mandrake_root.right
            else None)} \
        --inscribeGreaterWardingCircle ~{consecrated_chalk_powder.left} ~{
            (if (length(spell_components) != 0)
            then consecrated_chalk_powder.right
            else None)} \
        --dispelLingering ~{warding_dust.left} ~{(if (length(
            spell_components
        ) != 0)
            then warding_dust.right
            else None)} \
    >>>
}
