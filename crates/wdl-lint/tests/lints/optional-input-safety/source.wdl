#@ except: MetaDescription, ExpectedRuntimeKeys, ParameterMetaMatched, HereDocCommands, ShellCheck, ParameterDescription, MetaSections, RequirementsSection, ContainerUri, MatchingOutputMeta, DocMetaStrings, DescriptionLength, CallInputKeyword, SectionOrdering, ConsistentNewlines, CommandSectionIndentation, DoubleQuotes, RuntimeSection, OutputName, InputName, SnakeCase, PascalCase, DeclarationName, ImportPlacement, TodoComment, UnusedDocComments, ConciseInput, RedundantNone, DeprecatedPlaceholder, DeprecatedObject, ExceptDirectiveValid, KnownRules

## OptionalInputSafety (#707 / RFC #707): guarded vs unguarded optional placeholders in task commands.

version 1.1

# Bad: bare optional in command (RFC bad example; unquoted).
task bad_unquoted_optional {
    meta {}

    parameter_meta {}

    input {
        String? optional_flag
        File input_bam
    }

    command <<<
        set -euo pipefail
        samtools sort ~{optional_flag} ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Bad: optional inside shell quotes — same rule (still no explicit None guard in WDL).
task bad_quoted_optional {
    meta {}

    parameter_meta {}

    input {
        String? optional_flag
        File input_bam
    }

    command <<<
        set -euo pipefail
        samtools sort "~{optional_flag}" ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Good: if/else with defined() and string concatenation (RFC good example).
task good_if_defined_concat {
    meta {}

    parameter_meta {}

    input {
        String? output_path
        File input_bam
    }

    command <<<
        set -euo pipefail
        samtools sort ~{if defined(output_path) then "-o " + output_path else ""} ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Good: select_first with default (RFC good example).
task good_select_first_default {
    meta {}

    parameter_meta {}

    input {
        String? output_path
        File input_bam
    }

    command <<<
        set -euo pipefail
        samtools sort -o ~{select_first([output_path, "default.bam"])} ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Good: literal concatenation where optional only appears inside select_first.
task good_concat_select_first {
    meta {}

    parameter_meta {}

    input {
        String? flag
        File input_bam
    }

    command <<<
        set -euo pipefail
        samtools sort ~{"--flag " + select_first([flag, "default"])} ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Good: select_first on optional Int (stdlib select_all requires non-optional Array[X]).
task good_select_first_optional_int {
    meta {}

    parameter_meta {}

    input {
        Int? maybe_threads
    }

    command <<<
        set -euo pipefail
        echo ~{select_first([maybe_threads, 1])}
    >>>

    output {}

    runtime {}
}

# No warning: non-optional String in placeholder.
task no_warn_non_optional {
    meta {}

    parameter_meta {}

    input {
        String required_flag = "x"
        File input_bam
    }

    command <<<
        echo ~{required_flag} ~{input_bam}
    >>>

    output {}

    runtime {}
}

# Bad: optional used in addition without guard (both sides must be safe for +).
task bad_optional_plus_empty_string {
    meta {}

    parameter_meta {}

    input {
        String? x
    }

    command <<<
        echo ~{x + ""}
    >>>

    output {}

    runtime {}
}

# Optional in workflow output expression (not a task command) — should not trigger OptionalInputSafety.
workflow wf_optional_in_output_only {
    input {
        String? maybe
    }

    output {
        String out = "~{maybe}"
    }
}
