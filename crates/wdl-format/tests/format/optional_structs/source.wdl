version 1.1
struct SpliceJunctionMotifs {
    Int noncanonical_motifs
    Int GT_AG_and_CT_AC_motif
}
workflow foo {
    SpliceJunctionMotifs declared = SpliceJunctionMotifs {
        noncanonical_motifs: 1,
        GT_AG_and_CT_AC_motif: 2
    }
    SpliceJunctionMotifs? optional = None
    Object? deprecated = None
    input {
        SpliceJunctionMotifs? foo
    }
    output {
        SpliceJunctionMotifs? bar = declared
    }
}