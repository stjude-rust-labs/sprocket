#!/bin/bash

set -euo pipefail

outputs=$1
bin_to_call=$2

sh "$TEST_BIN/$bin_to_call" "$outputs"
