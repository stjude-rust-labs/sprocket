version 1.2

task t {
    input {
        File remote1
        File remote2
        File remote3
        File remote4
        File remote5
        File remote6
        File remote7
        File remote8
        File remote9
        File remote10
        File remote11
        File remote12
        File remote13
        File remote14
        File remote15
        File remote16
        File remote17
        File remote18
        File remote19
        File remote20
        File remote21
        File remote22
        File remote23
        File remote24
        File remote25
        File remote26
        File remote27
        File remote28
        File remote29
        File remote30
        File remote31
        File remote32
        File remote33
        File remote34
        File remote35
        File remote36
        File remote37
        File remote38
        File remote39
        File remote40
        File remote41
        File remote42
        File remote43
        File remote44
        File remote45
        File remote46
        File remote47
        File remote48
        File remote49
        File remote50
        File local_file
    }

    File relative_path = "relative.txt"

    command <<<
        set -euo pipefail
        cat '~{remote1}' > remote1
        cat '~{remote2}' > remote2
        cat '~{remote3}' > remote3
        cat '~{remote4}' > remote4
        cat '~{remote5}' > remote5
        cat '~{remote6}' > remote6
        cat '~{remote7}' > remote7
        cat '~{remote8}' > remote8
        cat '~{remote9}' > remote9
        cat '~{remote10}' > remote10
        cat '~{remote11}' > remote11
        cat '~{remote12}' > remote12
        cat '~{remote13}' > remote13
        cat '~{remote14}' > remote14
        cat '~{remote15}' > remote15
        cat '~{remote16}' > remote16
        cat '~{remote17}' > remote17
        cat '~{remote18}' > remote18
        cat '~{remote19}' > remote19
        cat '~{remote20}' > remote20
        cat '~{remote21}' > remote21
        cat '~{remote22}' > remote22
        cat '~{remote23}' > remote23
        cat '~{remote24}' > remote24
        cat '~{remote25}' > remote25
        cat '~{remote26}' > remote26
        cat '~{remote27}' > remote27
        cat '~{remote28}' > remote28
        cat '~{remote29}' > remote29
        cat '~{remote30}' > remote30
        cat '~{remote31}' > remote31
        cat '~{remote32}' > remote32
        cat '~{remote33}' > remote33
        cat '~{remote34}' > remote34
        cat '~{remote35}' > remote35
        cat '~{remote36}' > remote36
        cat '~{remote37}' > remote37
        cat '~{remote38}' > remote38
        cat '~{remote39}' > remote39
        cat '~{remote40}' > remote40
        cat '~{remote41}' > remote41
        cat '~{remote42}' > remote42
        cat '~{remote43}' > remote43
        cat '~{remote44}' > remote44
        cat '~{remote45}' > remote45
        cat '~{remote46}' > remote46
        cat '~{remote47}' > remote47
        cat '~{remote48}' > remote48
        cat '~{remote49}' > remote49
        cat '~{remote50}' > remote50
        cat '~{local_file}' > ~{relative_path}
    >>>

    output {
        File out1 = "remote1"
        File out2 = "remote2"
        File out3 = "remote3"
        File out4 = "remote4"
        File out5 = "remote5"
        File out6 = "remote6"
        File out7 = "remote7"
        File out8 = "remote8"
        File out9 = "remote9"
        File out10 = "remote10"
        File out11 = "remote11"
        File out12 = "remote12"
        File out13 = "remote13"
        File out14 = "remote14"
        File out15 = "remote15"
        File out16 = "remote16"
        File out17 = "remote17"
        File out18 = "remote18"
        File out19 = "remote19"
        File out20 = "remote20"
        File out21 = "remote21"
        File out22 = "remote22"
        File out23 = "remote23"
        File out24 = "remote24"
        File out25 = "remote25"
        File out26 = "remote26"
        File out27 = "remote27"
        File out28 = "remote28"
        File out29 = "remote29"
        File out30 = "remote30"
        File out31 = "remote31"
        File out32 = "remote32"
        File out33 = "remote33"
        File out34 = "remote34"
        File out35 = "remote35"
        File out36 = "remote36"
        File out37 = "remote37"
        File out38 = "remote38"
        File out39 = "remote39"
        File out40 = "remote40"
        File out41 = "remote41"
        File out42 = "remote42"
        File out43 = "remote43"
        File out44 = "remote44"
        File out45 = "remote45"
        File out46 = "remote46"
        File out47 = "remote47"
        File out48 = "remote48"
        File out49 = "remote49"
        File out50 = "remote50"
        File relative_out = relative_path
    }
}

workflow test {
    input {
        File remote1
        File remote2
        File remote3
        File remote4
        File remote5
        File remote6
        File remote7
        File remote8
        File remote9
        File remote10
        File remote11
        File remote12
        File remote13
        File remote14
        File remote15
        File remote16
        File remote17
        File remote18
        File remote19
        File remote20
        File remote21
        File remote22
        File remote23
        File remote24
        File remote25
        File remote26
        File remote27
        File remote28
        File remote29
        File remote30
        File remote31
        File remote32
        File remote33
        File remote34
        File remote35
        File remote36
        File remote37
        File remote38
        File remote39
        File remote40
        File remote41
        File remote42
        File remote43
        File remote44
        File remote45
        File remote46
        File remote47
        File remote48
        File remote49
        File remote50
        File local_file
    }

    call t { input:
        remote1,
        remote2,
        remote3,
        remote4,
        remote5,
        remote6,
        remote7,
        remote8,
        remote9,
        remote10,
        remote11,
        remote12,
        remote13,
        remote14,
        remote15,
        remote16,
        remote17,
        remote18,
        remote19,
        remote20,
        remote21,
        remote22,
        remote23,
        remote24,
        remote25,
        remote26,
        remote27,
        remote28,
        remote29,
        remote30,
        remote31,
        remote32,
        remote33,
        remote34,
        remote35,
        remote36,
        remote37,
        remote38,
        remote39,
        remote40,
        remote41,
        remote42,
        remote43,
        remote44,
        remote45,
        remote46,
        remote47,
        remote48,
        remote49,
        remote50,
        local_file,
    }

    output {
        Object out1 = read_json(t.out1)
        Object out2 = read_json(t.out2)
        Object out3 = read_json(t.out3)
        Object out4 = read_json(t.out4)
        Object out5 = read_json(t.out5)
        Object out6 = read_json(t.out6)
        Object out7 = read_json(t.out7)
        Object out8 = read_json(t.out8)
        Object out9 = read_json(t.out9)
        Object out10 = read_json(t.out10)
        Object out11 = read_json(t.out11)
        Object out12 = read_json(t.out12)
        Object out13 = read_json(t.out13)
        Object out14 = read_json(t.out14)
        Object out15 = read_json(t.out15)
        Object out16 = read_json(t.out16)
        Object out17 = read_json(t.out17)
        Object out18 = read_json(t.out18)
        Object out19 = read_json(t.out19)
        Object out20 = read_json(t.out20)
        Object out21 = read_json(t.out21)
        Object out22 = read_json(t.out22)
        Object out23 = read_json(t.out23)
        Object out24 = read_json(t.out24)
        Object out25 = read_json(t.out25)
        Object out26 = read_json(t.out26)
        Object out27 = read_json(t.out27)
        Object out28 = read_json(t.out28)
        Object out29 = read_json(t.out29)
        Object out30 = read_json(t.out30)
        Object out31 = read_json(t.out31)
        Object out32 = read_json(t.out32)
        Object out33 = read_json(t.out33)
        Object out34 = read_json(t.out34)
        Object out35 = read_json(t.out35)
        Object out36 = read_json(t.out36)
        Object out37 = read_json(t.out37)
        Object out38 = read_json(t.out38)
        Object out39 = read_json(t.out39)
        Object out40 = read_json(t.out40)
        Object out41 = read_json(t.out41)
        Object out42 = read_json(t.out42)
        Object out43 = read_json(t.out43)
        Object out44 = read_json(t.out44)
        Object out45 = read_json(t.out45)
        Object out46 = read_json(t.out46)
        Object out47 = read_json(t.out47)
        Object out48 = read_json(t.out48)
        Object out49 = read_json(t.out49)
        Object out50 = read_json(t.out50)
        String relative_out = read_string(t.relative_out)
    }
}
