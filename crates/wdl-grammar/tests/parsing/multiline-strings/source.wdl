## This is a test of multi-line strings from WDL 1.2

version 1.3

workflow test {
    String a = <<<
        Hello! This is a multi-line string!
        We can have line continuations \
        And escaped \>>>!
        But also use either ${value} or ~{value} for interpolations
    >>>
}
