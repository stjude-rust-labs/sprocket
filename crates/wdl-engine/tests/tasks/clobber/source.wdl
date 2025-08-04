version 1.2

task clobber {
    command <<<
        echo foo > foo.txt
        echo bar > foo.txt
    >>>
}