version 1.3

task clobber {
    command <<<
        echo foo > foo.txt
        echo bar > foo.txt
    >>>
}