# Slug: Performance testing data processing tool

## Overview

Slug is Rust based tool for storing and processing output data from various performance testing libraries for various languages. This tool aims to seamlessly integrate with said libraries, collect test results and perform statistical analysis on the gathered data. By monitoring performance metrics the tool will be looking for significant deviations from the norm, providing users with quick alerts for potential issues. The primary goal is to create an easy to use, reliable and high-performance tool.

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
The binary will be located in `target/release/`.


### Alternatively, you can directly run the project using Cargo:
```Bash
cargo run -- [OPTIONS]
```

## Examples
```Bash
cargo run -- -f examples/pytest-7.3.0 -t pytest-7.3.0
```