## This is a test of an unused file input in workflows and tasks.
##
## `wdl-analysis` used to follow the behavior of `miniwdl`, which would
## ignore `File` inputs with specific suffixes that only existed to localize
## the files to the same directory.
##
## This was reverted after https://github.com/stjude-rust-labs/sprocket/issues/218

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
