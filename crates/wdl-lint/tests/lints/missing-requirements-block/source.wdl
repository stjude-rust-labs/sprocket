#@ except: DeprecatedRuntimeSection, MetaDescription, EmptyOutputs, BashSetSyntax

version 1.3

task bad {
    meta {}

    command <<<>>>

    output {}
}

task good {
    meta {}

    command <<<>>>

    output {}

    requirements {
    }
}

task runtime_section {
   meta {}

   command <<<>>>

   output {}
   
   # This `runtime` section should suppress `RequirementsSection`
   runtime {
   }
}
