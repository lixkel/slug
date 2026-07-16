Slug is a Rust based tool for storing and processing output data from various performance testing libraries for various languages. It parses the output of a performance benchmark, records it in your project's Git repository, and runs statistical checks against the history of that benchmark to decide whether the latest measurement is a performance regression.

Slug is built to live in a CI pipeline. It reads benchmark output on stdin or from a file, needs no database or server of its own, and reports its verdict through its exit code so the surrounding pipeline can gate a merge on it.

## Where is the data stored?

Measurements are stored in Git notes, keyed by the commit they were measured on. The performance history is versioned and travels with the repository the same way the code does, so it is available to everyone who clones it rather than living on one machine. Because the data sits under `refs/notes/*` rather than on a branch, it never shows up in your branch list or in the hosting platform's web UI, and a push of the data does not retrigger `on: push` workflows.

There are two separate stores:

| Store | Flag | Reference | Purpose |
| --- | --- | --- | --- |
| Local | `--local` (default) | `refs/notes/slug-local` | Experiments on your machine, never pushed. |
| Shared | `--shared` | `refs/notes/slug-shared` | Team's history, pushed back by CI. |

Git does not fetch or push notes by default, so any pipeline moving shared history must name the reference explicitly. The templates in `templates/ci/` already do this.

> ⚠ slug has to be run inside git repository where you want the data to be saved

## Build

To build Slug, you need Rust installed.

```Bash
git clone https://github.com/lixkel/slug.git
cd slug
cargo build --release
```

The binary will be located in `target/release/slug`. During development you can also run it directly with `cargo run -- [OPTIONS]`.

## Usage

```
slug -t LIBRARY [-f FILE] [--local | --shared] [--record]
slug history [TEST] [--local | --shared]
slug setup
slug clean [--local | --shared]
slug prune
```

`LIBRARY` is given as `name@version`, for example `pytest@7.3.0`, a bare `pytest` selects the newest available parser. Versions resolve to the closest parser that is less than or equal to the one you asked for, so a parser keeps working for later releases whose output format has not changed.

Input is read from `-f FILE`, or from stdin when `-f` is omitted.

By default a run is a **dry run**: Slug parses the input and reports the verdict, but stores nothing. Pass `--record` to write the measurement into the history.

### Exit codes

Slug distinguishes "the build failed but the measurement is good" from "the run produced nothing trustworthy", which is why there are three codes rather than two:

| Code | Meaning |
| --- | --- |
| `0` | Clean run, no regression. |
| `2` | Regression detected. The measurement was still recorded and should be pushed. |
| `1` | An actual error (unparsable input, missing file). Nothing may be pushed. |

### Subcommands

- `history [TEST]` — print the recorded history as CSV, optionally for a single benchmark.
- `setup` — write commented example `slug.toml` to the repository root.
- `clean` — delete all Slug history in the selected store, after confirmation.
- `prune` — drop records attached to commits that are no longer reachable, for example after a branch was deleted or reset.

### Examples

```Bash
# Load data from file
slug -f examples/pytest@7.3.0 -t pytest@7.3.0

# Load data from pipe and record it into the shared history
pytest-benchmark | slug -t pytest --shared --record

# Print the recorded history as CSV
slug history
```

## Configuration

Configuration is optional. Without a config file Slug runs the `prediction-bound` check with its defaults. To tune it, put a `slug.toml` in the repository root by runnning `slug setup` which generates the default config. Every field has a default, so the file may be partial, but unknown keys are rejected.

```toml
# Which checks to run, and how to combine their verdicts.
enabled = ["prediction-bound"]
# any = flag if ANY of the enabled checks flag
# all = flag only if ALL checks flag
policy = "any"

# No check judges until its window has this many points.
min_samples = 10

[prediction-bound]
# Flag a new measurement above this share (level) of healthy measurements.
# The false alarm rate is one minus level: at 0.999, one healthy commit in 1000.
level = 0.999
window = 100

[zscore]
# Flag when the value is this many standard deviations above the mean.
threshold = 3.0
window = 100
```

Two checks are available:

- **`prediction-bound`** (default) — flags measurement that lands above the given share of the healthy distribution. `level` sets the false alarm rate directly, which makes it the easier knob to reason about.
- **`zscore`** — flags measurement more than `threshold` standard deviations above the mean of its window. Add `"zscore"` to `enabled` to turn it on.

Until a benchmark's window holds `min_samples` points, the checks report `skip` rather than guessing from too little data.

## CI/CD integration

Pipeline distinguishes two events, and Slug should not do the same thing on both:

- **On a pull request**, the commits are only a proposal and may be reworked or never merged. Slug measures, compares against the history inherited from the target branch, and lets the verdict gate the merge, but records nothing.
- **On a push**, the commits have actually entered a branch, so the measurement is recorded and the notes reference is pushed back.

`templates/ci/` contains prepared GitHub composite action and an example workflow that package this, they fetch the notes reference, run Slug, and push the reference back when the exit code allows it (`0` or `2`).

```yaml
- name: "Track Benchmarks with Slug"
  uses: "lixkel/slug-action@v1"
  with:
    file: "benchmark_output.txt"
    type: "pyperf@2.7.0"
```

There is one think worflow cannot forget about:

- **Full history.** Set `fetch-depth: 0` on `actions/checkout`; a shallow checkout hides the history the checks judge against.

The same recipe transfers to other platforms, which offer equivalent serialization primitives (GitLab CI has `resource_group`, Jenkins its lock plugins), though the push step needs credentials arranged per platform.

## Currently supported libraries

benchmarkdotnet, criterion, go-testing, google-benchmark, jmh, pyperf, pytest

New parsers are registered with the `add_libs!` macro in `src/parser.rs` and new statistical checks with `add_stat_checks!` in `src/statistics.rs`.