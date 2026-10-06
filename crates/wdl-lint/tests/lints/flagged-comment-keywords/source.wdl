#@ except: MetaDescription, MetaSections, RuntimeSection, EmptyOutputs, BashSetSyntax, DescriptionLength, RequirementsSection

version 1.3

# FIXME: at the start of the comment
# in the middle: XYZ (the keyword is not at either end)
# at the end of the comment XYZ
# multiple in one comment: XYZ FIXME XYZ
# overlapping keywords are reported once: FIXME
# a shorter overlapping keyword is still reported on its own: FIX
# matching is case-sensitive: Fixme fixme xxx
# TODO: this is not a configured keyword, so it is only flagged by default

workflow test {
    #@ except: FlaggedComment
    meta {
        # FIXME: this should NOT be flagged
    }

    output {}
}

task test2 {
    command <<<>>>
    # XYZ: trailing comment
}
