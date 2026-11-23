version 1.4

task clobber {
    command <<<
        echo foo > foo.txt
        echo bar > foo.txt
    >>>
}