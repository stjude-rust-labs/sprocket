#@ except: DescriptionMissing, RuntimeSectionKeys, MatchingParameterMeta, NoCurlyCommands

## This is a test of having shellcheck warnings

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command <<<
      somecommand.py $line17 ~{placeholder}
      somecommand.py ~{placeholder} $line18
      somecommand.py ~{placeholder}$line19










      somecommand.py $line30~{placeholder}
      somecommand.py [ -f $line31 ] ~{placeholder}
    >>>

    output {}

    runtime {}
}

task test2 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command {
      somecommand.py $line49 ~{placeholder}
      somecommand.py ~{placeholder} $line50
      somecommand.py ~{placeholder}$line51
      somecommand.py $line52~{placeholder}
      somecommand.py [ -f $bad_test ] ~{placeholder}
      somecommand.py [ -f $trailing_space ] ~{placeholder}
    }

    output {}

    runtime {}
}

task test3 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command <<<           # weird whitespace
      somecommand.py $line72 ~{placeholder}
      somecommand.py ~{placeholder} $line73
      somecommand.py ~{placeholder}$line74
      somecommand.py $line75~{placeholder}
      ~{placeholder} $line76_trailing_pholder ~{placeholder}
      ~{placeholder} somecommand.py $leading_pholder
    >>>

    output {}

    runtime {}
}

task test4 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command <<<
            # other weird whitspace
      somecommand.py $line96 ~{placeholder}
      somecommand.py ~{placeholder} $line97
      somecommand.py ~{placeholder}$line98
      somecommand.py $line99~{placeholder}
      ~{placeholder} $line100_trailing_pholder ~{placeholder}
      ~{placeholder} somecommand.py $leading_pholder
    >>>

    output {}

    runtime {}
}

task test5 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command <<<      weird stuff $firstlinelint
            # other weird whitespace
      somecommand.py $line120 ~{placeholder}
      somecommand.py ~{placeholder} $line121
      somecommand.py ~{placeholder}$line122
      somecommand.py $line123~{placeholder}
      ~{placeholder} $line124_trailing_pholder ~{placeholder}
      ~{by + myself}
      ~{placeholder} somecommand.py $leading_pholder

        ~{
          multiline +
          placeholder
        }
      $occurs_after_multiline

      $(echo This is a 
        very long string that should be quoted)
      
      $(echo This is an
        even longer very long string that should really 
        be quoted)
      
      $(echo This is an
        even longer very long string that should really
        really really really 
        ought to be quoted)

      $(echo this is a $lint146 that occurs in a \
        multiline command \
        with line breaks)
    >>>

    output {}

    runtime {}
}
