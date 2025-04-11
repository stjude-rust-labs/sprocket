#@ except: MetaDescription, ExpectedRuntimeKeys, ParameterMetaMatched, HereDocCommands

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

task test6 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command <<<
      version=`uname -r`

      cd "DIR"
    >>>

    output {}

    runtime {}
}

task test6 {
    meta {}

    parameter_meta {}

    input {}

    command <<<
        convert_fusions_to_vcf.sh \
            $fasta_name \
            ~{fusions} \
            ~{prefix}.vcf

        for file in ~{sep(" ", bams)}
        do
          # This will fail (intentionally) if there are duplicate names
          # in the input BAM array.
          ln -s $file
          bams+=" $(basename $file)"
        done
        
        if ! ~{succeed_on_errors} \
            && [ "$(grep -Ec "$GREP_PATTERN" $outfile_name)" -gt 0 ]
        then
            >&2 echo "Problems detected by Picard ValidateSamFile"
            >&2 grep -E "$GREP_PATTERN" ~{outfile_name}
            exit $rc
        fi



    >>>

    output {}

    runtime {}
}
