#!/bin/bash

set -u

PYTHON_VERSION="3.10.0"
REPO_URL="https://github.com/html5lib/html5lib-python.git"
DIR_NAME="html5lib-python"
OUTPUT_FILE="benchmark_output.txt"
SLUG_BINARY="$(readlink -f ../target/release/slug)"
NUM_COMMITS=30

GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[1;34m'
NC='\033[0m'

# Start timer
START_TIME=$SECONDS

echo -e "${GREEN}Using Slug binary $SLUG_BINARY${NC}"

# Setup html5lib
if [ -d "$DIR_NAME" ]; then
    echo -e "${BLUE}Cleaning up existing directory $DIR_NAME${NC}"
    rm -rf "$DIR_NAME"
fi

echo -e "${GREEN}Cloning repository${NC}"
git clone "$REPO_URL" "$DIR_NAME"
cd "$DIR_NAME"

# Setup python version using pyenv
PYTHON_BIN="$(pyenv root)/versions/$PYTHON_VERSION/bin/python"
if [ ! -x "$PYTHON_BIN" ]; then
    echo -e "${BLUE}Installing Python $PYTHON_VERSION using pyenv${NC}"
    pyenv install -s "$PYTHON_VERSION"
fi

echo -e "${GREEN}Creating python venv ($PYTHON_VERSION)${NC}"
"$PYTHON_BIN" -m venv venv
source venv/bin/activate

# Benchmark runtime dependencies
pip install --only-binary=:all: pyperf==2.7.0 lxml==6.1.1

# Cleanup
trap "git checkout master > /dev/null 2>&1" EXIT

echo -e "${GREEN}Benchmarking on last $total_commits commits ${NC}"

# Time Travel Loop
# Get last N commits in reversed order
commits=$(git rev-list HEAD -n "$NUM_COMMITS" | tac)
total_commits=$(echo "$commits" | wc -w)
current_count=0
for commit in $commits; do
    current_count=$((current_count + 1))
    echo -e "\n${BLUE}[$current_count/$total_commits] Processing commit: $commit${NC}"

    # Checkout commit
    git checkout -f "$commit" > /dev/null 2>&1

    # Install library versions from this commit
    if pip install -e . > /dev/null 2>&1; then

        # Run benchmark
        if [ -f "benchmarks/bench_html.py" ]; then
            echo "Running benchmark..."

            if python benchmarks/bench_html.py parse > "$OUTPUT_FILE"; then

                # Feed the Slug
                echo -e "${GREEN}Benchmark success. Feeding the Slug...${NC}"
                "$SLUG_BINARY" -f "$OUTPUT_FILE" -t pyperf@2.7.0
            else
                echo -e "${RED}Benchmark failed${NC}"
            fi
        else
            echo -e "${RED}Skipping: Benchmark script missing in this commit${NC}"
        fi
    else
        echo -e "${RED}Skipping: Failed to install dependencies for this commit${NC}"
    fi
done

ELAPSED_TIME=$((SECONDS - START_TIME))
echo -e "\n${GREEN}Done! Total time: ${ELAPSED_TIME}s${NC}"