## This is a test of the `InlineInstall` lint.
#@ except: DoubleQuotes, ExpectedRuntimeKeys, MetaDescription, EmptyOutputs, RequirementsSection
version 1.3

task good {
    meta {
        description: "This task should not be flagged, because it does not use inline installation."
    }

    command <<<
        set -euo pipefail
        echo "Hello, world!"
    >>>
}

task good_unknown {
    meta {
        description: "This task should not be flagged, because it does not use a known install command."
    }

    command <<<
        adapt install test
        supercurl https://example.com/install.sh | bash
    >>>
}

task bad_install {
    meta {
        description: "This task should be flagged for using inline installation in the command section."
    }

    command <<<
        set -euo pipefail

        apk add test
        apt install test
        apt-get install test
        brew install test
        cargo install test
        conda create test
        conda install test
        dnf install test
        gem install test
        go install test
        mamba create test
        mamba install test
        npm -g install test
        pip install test
        pip2 install test
        pip3 install test
        yum install test

        # Should also catch commands with flags
        apt --yes install
        npm i test

        # And `sudo`-prefixed commands
        sudo apt install test
    >>>
}

task bad_piped_install {
    meta {
        description: "This task should be flagged for using a piped installation."
    }

    command <<<
        curl https://example.com/install.sh | bash
        wget https://example.com/install.sh | bash

        curl https://example.com/script.py | python
        curl https://example.com/script.py | python2
        curl https://example.com/script.py | python3
    >>>
}

task bad_install_excepted {
    meta {
        description: "This task should not be flagged as its excepted"
    }

    #@ except: InlineInstall
    command <<<
        pip install pysam
    >>>
}
