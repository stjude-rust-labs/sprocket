#@ except: MetaDescription, RuntimeSection
#@ except: MetaSections

version 1.0

workflow test {
    input {
        String arm
        String cam
        Int bam
    }

    # This should not flag any notes, since version is 1.0
    call bar { input:
        arm = arm,
        bam = bam + 3,
        cam = cam,
   }
}

task bar {
    input {
        String arm
        String cam
        Int bam
    }

    command <<<>>>
}
