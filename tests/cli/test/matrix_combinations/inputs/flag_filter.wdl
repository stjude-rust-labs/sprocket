## copied from github.com/stjudecloud/workflows

version 1.1

struct FlagFilter {
    String include_if_all  # samtools -f
    String exclude_if_any  # samtools -F
    String include_if_any  # samtools --rf
    String exclude_if_all  # samtools -G
}
