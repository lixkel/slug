#!/bin/bash

set -euo pipefail

INPUT_FILE="pytest-7.3.0-template"
OUTPUT_FILE="pytest-7.3.0-template-tmp"

MIN="10.0"
MAX="20.0"
MEAN="15.0"
STDDEV="2.5"
MEDIAN="14.0"
IQR="5.0"
OUTLIERS="1"
OPS="66.67"
ROUNDS="30"
ITERATIONS="1000"

fake_test() {
    sed -e "s/{{MIN}}/$MIN/g" \
        -e "s/{{MAX}}/$MAX/g" \
        -e "s/{{MEAN}}/$MEAN/g" \
        -e "s/{{STDDEV}}/$STDDEV/g" \
        -e "s/{{MEDIAN}}/$MEDIAN/g" \
        -e "s/{{IQR}}/$IQR/g" \
        -e "s/{{OUTLIERS}}/$OUTLIERS/g" \
        -e "s/{{OPS}}/$OPS/g" \
        -e "s/{{ROUNDS}}/$ROUNDS/g" \
        -e "s/{{ITERATIONS}}/$ITERATIONS/g" \
        "../$INPUT_FILE" > "$OUTPUT_FILE"

    echo "Placeholders replaced and output saved to $OUTPUT_FILE"

    MIN=$(echo "$MIN + 1" | bc)
    MAX=$(echo "$MAX + 1" | bc)
    MEAN=$(echo "$MEAN + 100" | bc)
    STDDEV=$(echo "$STDDEV + 0.1" | bc)
    MEDIAN=$(echo "$MEDIAN + 1" | bc)
    IQR=$(echo "$IQR + 0.5" | bc)
    #OUTLIERS=$(echo "$OUTLIERS + 1" | bc)
    OPS=$(echo "$OPS + 1" | bc)
    ROUNDS=$(echo "$ROUNDS + 1" | bc)
    ITERATIONS=$(echo "$ITERATIONS + 100" | bc)
}

create_commits() {
    local num_commits=$1
    local file_name=$2

    cur_branch=$(git branch --show-current)
    for i in $(seq 1 $num_commits); do
        echo "Commit $i on branch $cur_branch" >> "$FILE_NAME"
        git add "$FILE_NAME"
        git commit -m "Commit $i on branch $cur_branch"
        fake_test
        ../../target/release/slug -f "$OUTPUT_FILE" -t pytest-7.3.0
    done
}

create_branch_commits() {
    local branch="$1"
    local num_commits="$2"
    local file_name="$3"

    git checkout -b "$branch" 

    create_commits "$num_commits" "$FILE_NAME"
}

DIR_NAME="new_git_repo"

if [ -d "$DIR_NAME" ]; then
  rm -rf "$DIR_NAME"
fi

mkdir "$DIR_NAME"
cd "$DIR_NAME"

git init


# Create an init file and init commit
FILE_NAME="file.txt"
echo "Initial commit" > "$FILE_NAME"
git add "$FILE_NAME"
git commit -m "Initial commit"

# create new branches
create_branch_commits "new-feature" 3 "$FILE_NAME"
create_branch_commits "bug-fix" 3 "$FILE_NAME"


git checkout master
create_commits 3 "$FILE_NAME"

# Return to the main branch
git checkout master

echo "Git repository setup with branches and commits is complete."
