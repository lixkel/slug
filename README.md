# Slug: Performance testing data processing tool

## Overview

Slug is Rust based tool for storing and processing output data from various performance testing libraries for various languages. This tool aims to seamlessly integrate with said libraries, collect test results and perform statistical analysis on the gathered data. By monitoring performance metrics the tool will be looking for significant deviations from the norm, providing users with quick alerts for potential issues. The primary goal is to create an easy to use, reliable and high-performance tool.

## Where is the data stored?

The historical data of tests are stored inside your git repository so that they are available wherever you need them not just on your local machine. Data can either be stored in its own separate parallel git tree or in each branch in .slug directory using `commit --amend`. The default option is in its own separate git tree. If you want to use the amend option use `-a` flag

> ⚠ slug has to be run inside git repository where you want the data to be saved

> ⚠ slug will delete all your local unsaved changes (this will be fixed)

## Future features

- **Integration with Multiple Libraries**: Easily connects with various performance testing libraries.
- **Data Collection and Processing**: Gathers and processes test results.
- **Statistical Analysis**: Performs comprehensive statistical analysis on test data.
- **Performance Monitoring**: Monitors key performance metrics and identifies significant deviations.
- **Alert System**: Provides immediate alerts for potential performance issues.

## Build

To install Slug, you need to have Rust installed on your system. Follow these steps:

1. Clone the repository:
```Bash
git clone https://gitlab.mff.cuni.cz/teaching/nprg045/horky/mojikm/implementation.git
```

2. Navigate to the project directory:
```Bash
cd slug
```

3. Build the project using Cargo:
```Bash
cargo build --release
```
The binary will be located in `target/release/slug`.


### Alternatively, you can directly run the project using Cargo:
```Bash
cargo run -- [OPTIONS]
```

## Examples

#### Load data from file
```Bash
slug -f examples/pytest@7.3.0 -t pytest@7.3.0
```

#### Load data from pipe
```Bash
pytest-benchmark | slug -t pytest@7.3.0
```

## Currently supported libraries:

pytest