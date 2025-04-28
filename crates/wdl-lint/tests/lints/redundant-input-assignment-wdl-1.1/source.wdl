#@ except: MetaDescription, ExpectedRuntimeKeys, OutputSection, MetaSections

version 1.1

workflow test1 {
    input {
        String arm
        String cam
        Int bam
    }

    call bar { input:
         arm,  # should not flag
         bam = bam + 3,  # should not flag
         cam = cam,  # This should flag a note, since version is >= 1.1
    }

    #@ except: ConciseInput
    call bar as bar2 { input:
         arm,  # should not flag
         bam = bam + 3,  # should not flag
         cam = cam,  # This should not flag a note due to the except statement
    }

    call bar as bar3 { input:
        #@ except: ConciseInput
        arm = arm,  # should not flag a note due to the except statement
        bam = bam,  # should flag a note
   }
}

#@ except: RuntimeSection
task bar {
    input {
        String arm
        Int bam
        String? cam
    }

    command <<<>>>
}
