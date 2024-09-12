#!/bin/bash

set -euo pipefail

OUTPUT_FILE="html5lib-tmp"

git clone https://github.com/html5lib/html5lib-python.git
cd html5lib-python
virtualenv env
source env/bin/activate
pip install -r requirements.txt
pip install pyperf
pip install lxml

commits=$(git rev-list HEAD -n 20 | tac)

for commit in $commits; do
    git checkout $commit

    python benchmarks/bench_html.py parse >"$OUTPUT_FILE"
    ../../target/release/slug -f "$OUTPUT_FILE" -t pyperf-2.7.0
done