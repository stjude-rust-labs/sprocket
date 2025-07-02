---
layout: page
sidebar: false
---
<script setup>
    import HomePage from './.vitepress/theme/components/Homepage.vue'
</script>

<HomePage>

```wdl
version 1.0

workflow count_lines {
    input { File input_file }
    call Count { input: file = input_file }
    output { Int num_lines = Count.num_lines }
}

task Count {
    input { File file }
    command { wc -l ${file} | awk '{print $1}' }
    output { Int num_lines = read_int(stdout()) }
}
```

</Homepage>