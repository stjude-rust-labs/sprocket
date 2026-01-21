#@ except: DeprecatedObject, MetaDescription, InputSorted
#@ except: ParameterMetaMatched, MetaSections, RequirementsSection
#@ except: DocMetaStrings, ParameterDescription

version 1.3

workflow bar {
    call foo {
        bam = "test.bam",
        gtf = "test.gtf",
        strandedness = "yes",
        minaqual = 10,
        modify_memory_gb = 2  # some other junk
        ,
        modify_disk_size_gb = 2,
        not_an_option = "test",
    }

    call foo as foo2 {
        bam = "test.bam",
        gtf = "test.gtf",
        strandedness = "yes",
        minaqual = 10,
        modify_memory_gb = 2,
        modify_disk_size_gb = 2,
        not_an_option = "test"
    }
}

task foo {
    meta {
        description: {
            help: "test"  # OK
        }
        help: {
            name: "something",
            other: "another"  # missing comma
        }
        foo: {
            bar: "baz",
            baz: "quux" ,  # misplaced comma
        }
        bar: {
            baz: "quux",
            quux: "quuz",  # OK
        }
        baz: {
            bar: "baz",
            baz: "quux"  # wow this is ugly
            # technically legal!
            ,  # comments are horrible!
        }
    }

    parameter_meta {
        bam: "Input BAM format file to generate coverage for"
        gtf: "Input genomic features in gzipped GTF format to count reads for"
        strandedness: {
            description: "Strandedness protocol of the RNA-Seq experiment",
            external_help: "https://htseq.readthedocs.io/en/latest/htseqcount.html#cmdoption-htseq-count-s",
            choices: [
                "yes",
                "reverse",
                "no"  # missing comma
            ]  # missing comma
        }
        minaqual: {
            description: "Skip all reads with alignment quality lower than the given minimum value",
            common: true  # missing comma
        }
        modify_memory_gb: "Add to or subtract from dynamic memory allocation. Default memory is determined by the size of the inputs. Specified in GB."
        modify_disk_size_gb: "Add to or subtract from dynamic disk space allocation. Default disk size is determined by the size of the inputs. Specified in GB."
        not_an_option: {
            name: "test"  # OK
        }
   }

   input {
         String bam
         String gtf
         String strandedness
         Int minaqual
         Int modify_memory_gb
         Int modify_disk_size_gb
         String not_an_option
         Array[Int] another = [1,2,3]
         Array[Int] another2 = [
            1,
            2,
            3
        ]
   }

    Map[String, String] ano = {
        "a": "b",
        "c": "d"
    }

    Object q = {
        "a": "b",
        "c": "d"
    }

   command <<< >>>

   output {}

   runtime {}
}
