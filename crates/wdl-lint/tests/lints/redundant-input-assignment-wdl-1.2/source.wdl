#@ except: MetaDescription, RequirementsSection
#@ except: ExpectedRuntimeKeys, MetaSections, OutputSection

version 1.2

workflow test1 {
    input {
        String arm
        String cam
        Int bam
    }

    # This should flag, since version >= 1.1 and there are redundant input assignments
    # This test was created to ensure the rule works without the explicit "input"
    call bar {
         arm,  # should not flag
         bam = bam + 3,  # should not flag
         cam = cam,  # should flag
   }
}

workflow test2 {
    input {
        String arm
        String cam
        Int bam
    }

    #@ except: ConciseInput
    call bar {
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

    call foo {
        #@ except: ConciseInput
        arm = arm,  # should not flag a note due to the except statement
        bam = bam,  # should flag a note
   }
}
