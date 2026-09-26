## This is a test of the `ShellSplitting` rule

#@ except: BashSetSyntax, EmptyOutputs, ExpectedRuntimeKeys, HereDocCommands
#@ except: MetaDescription, ParameterMetaMatched, ShellCheck

version 1.2

task unquoted {
    meta {}

    parameter_meta {}

    input {
        File bam
        String? region
        String prefix
        Directory reference
        Array[File] files
    }

    command <<<
        # Each of these is split.
        samtools index ~{bam}
        samtools view --output=~{prefix}.bam
        ls ~{reference}/*
        cat ~{sep(" ", files)}
        cat "~{bam}" ~{if defined(region) then "--region " + region else ""}
        echo $(basename ~{bam})
        echo "$(basename ~{bam})"
        echo `basename ~{bam}`
        [ -f ~{bam} ]
        wc -l <<< ~{prefix}
        cat > ~{prefix}.txt
        arr=(~{prefix} "~{bam}")
        for f in ~{sep(" ", files)}; do echo "$f"; done
        echo "~{bam}"~{prefix}"~{bam}"
    >>>

    output {}

    requirements {}
}

task quoted {
    meta {}

    parameter_meta {}

    input {
        File bam
        String? region
        String prefix
        Array[File] files
        Int threads = 4
        Boolean flag = true
    }

    command <<<
        # None of these are split.
        samtools index "~{bam}"
        samtools view '~{bam}'
        samtools view $'~{bam}'
        samtools view "--output=~{prefix}.bam"
        samtools view ~{threads} ~{flag} ~{threads + 1}
        samtools view ~{if flag then "--flag" else ""}
        samtools view ~{if threads > 1 then "--threads " + (threads - 1) else ""}
        samtools view ~{if defined(region) then "--region '~{region}'" else ""}
        samtools view ~{if defined(region) then "--region " + "'" + region + "'" else ""}
        cat ~{sep(" ", squote(files))}
        cat ~{sep(" ", quote(files))}
        cat ~{sep(" ", prefix("-I '", suffix("'", files)))}
        cat "~{sep(" ", files)}"
        x=~{bam}
        export y=~{bam} z=~{prefix}
        local w=~{bam}
        [[ -f ~{bam} ]]
        case ~{prefix} in
            ~{prefix}) echo "match" ;;
        esac
        echo $(( ~{threads} + 1 ))
        # a comment about ~{bam}
        cat <<EOF
        ~{bam}
        EOF
        echo "$(basename "~{bam}")"
        echo ~{sep("", ["'", prefix, "'"])}
        2>/dev/null v=~{bam} env
        cat <<'END MARK'
        ~{bam}
        END MARK
        cat <<EOF
         EOF
        ~{bam}
        EOF
    >>>

    output {}

    requirements {}
}

# https://github.com/stjude-rust-labs/sprocket/issues/605
task issue_605 {
    meta {}

    parameter_meta {}

    input {
        File bam
    }

    Int threads = 4

    command <<<
        samtools index --bai \
            ~{if threads > 1 then "--threads " + (threads - 1) else ""} \
            --output ~{bam}.bai \
            ~{bam}
    >>>

    output {}

    requirements {}
}

# https://github.com/stjude-rust-labs/sprocket/issues/823
task issue_823 {
    meta {}

    parameter_meta {}

    input {
        Boolean output_fastq
        File? read_two_fastq
        String prefix
    }

    command <<<
        fastp \
            ~{if output_fastq
                then "-o '" + if defined(read_two_fastq)
                    then "~{prefix}.R1.fastq.gz'"
                    else "~{prefix}.fastq.gz'"
                else ""
            } \
            ~{if output_fastq
                then "-o " + if defined(read_two_fastq)
                    then "~{prefix}.R1.fastq.gz"
                    else "'~{prefix}.fastq.gz'"
                else ""
            }
    >>>

    output {}

    requirements {}
}

# https://github.com/stjude-rust-labs/sprocket/issues/841
task issue_841 {
    meta {}

    parameter_meta {}

    input {
        Array[String] contigs
    }

    command <<<
        run_clair3.sh \
            ~{if length(contigs) > 0
                then "--ctg_name='~{sep(",", contigs)}'"
                else ""
            } \
            ~{if length(contigs) > 0
                then "--ctg_name=~{sep(",", contigs)}"
                else ""
            }
    >>>

    output {}

    requirements {}
}

task placeholder_options {
    meta {}

    parameter_meta {}

    input {
        String? name
        Array[String] names
        Boolean flag = true
    }

    command {
        echo ~{default="none" name}
        echo ~{default="two words" name}
        echo ~{true="yes" false="no" flag}
        echo ~{true="--flag on" false="" flag}
        echo ~{sep=" " names}
        echo "~{sep=" " names}"
    }

    output {}

    requirements {}
}

task excepted {
    meta {}

    parameter_meta {}

    input {
        File bam
    }

    #@ except: ShellSplitting
    command <<<
        samtools index ~{bam}
    >>>

    output {}

    requirements {}
}
