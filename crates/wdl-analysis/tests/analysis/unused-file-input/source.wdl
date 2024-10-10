## This is a test of an unused file input in workflows and tasks.

version 1.1

task test {
    input {
        # This is actually unused because it doesn't have matching suffix
        File used

        # The remainder are "used" because they have a matching suffix
        File used_index
        File used_indexes
        File used_indices
        File used_idx
        File used_tbi
        File used_bai
        File used_crai
        File used_csi
        File used_fai
        File used_dict
    }

    command <<<>>>
}

workflow wf {
    input {
        # This is actually unused because it doesn't have matching suffix
        File used

        # The remainder are "used" because they have a matching suffix
        File used_index
        File used_indexes
        File used_indices
        File used_idx
        File used_tbi
        File used_bai
        File used_crai
        File used_csi
        File used_fai
        File used_dict
    }
}
