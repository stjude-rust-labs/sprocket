version 1.3

task training {
    String training_dir = "training"

    command <<<
        echo ~{training_dir}
    >>>

    output {
        Array[File] training_dir = glob("~{training_dir}")
    }
}

workflow my_workflow {
    call training

    output {
        Array[File] final_output = training.training_dir
    }
}
