#@ except: DescriptionMissing, RuntimeSectionKeys, MatchingParameterMeta, NoCurlyCommands

## This is a test of having no shellcheck lints

version 1.1

task test1 {
    meta {}

    parameter_meta {}

    input {
      Boolean i_quote_my_shellvars
      Int placeholder
    }

    command <<<
      set -eo pipefail

      echo "$placeholder"

      if [[ "$i_quote_my_shellvars" ]]; then
        echo "shellcheck will be happy"
      fi
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
      set -eo pipefail

      echo "$placeholder"

      if [[ "$I_quote_my_shellvars" ]]; then
        echo "all is well"
      fi
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

    #@ except: ShellCheck
    command {
      set -eo pipefail

      echo "$placeholder"

      if [[ $I_really_want_this_unquoted ]]; then
        echo "all is not well"
      fi
    }

    output {}

    runtime {}
}

task test4 {
    meta {}

    parameter_meta {}

    input {
      Int placeholder
    }

    command {
      set -eo pipefail
      
      unquoted_var="foo bar baz"
      # shellcheck disable=SC2086
      echo $unquoted_var
    }

    output {}

    runtime {}
}
