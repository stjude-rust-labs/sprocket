## This is a test of multi-line strings from WDL 1.2
## There should be no diagnostics for this test.

version 1.3

workflow ok {
    String ok = <<<
        This is a multi-line string.
        It may contain either ${<<<dollar>>>} or ~{"tilde"} placeholders.
        It may contain line continuations \
        And escaped endings \>>>.
    >>>
}
