#@ except: MetaDescription, RequirementsSection
#@ except: ExpectedRuntimeKeys, OutputSection, MetaSections

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
}

workflow test2 {
    input {
        String arm
        String cam
        Int bam
    }

    #@ except: ConciseInput
    call bar { input:
         arm,  # should not flag
         bam = bam + 3,  # should not flag
         cam = cam,  # This should not flag a note due to the except statement
   }
}

workflow test3 {
    input {
        String arm
        Int bam
    }

    call foo { input:
        #@ except: ConciseInput
        arm = arm,  # should not flag a note due to the except statement
        bam = bam,  # should flag a note
   }
}
