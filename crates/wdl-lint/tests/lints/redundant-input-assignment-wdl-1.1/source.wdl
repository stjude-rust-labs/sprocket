#@ except: DescriptionMissing, MissingRequirements
#@ except: RuntimeSectionKeys, MissingOutput, MissingMetas

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

    #@ except: RedundantInputAssignment
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
        #@ except: RedundantInputAssignment
        arm = arm,  # should not flag a note due to the except statement
        bam = bam,  # should flag a note
   }
}
