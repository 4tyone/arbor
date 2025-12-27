<div align="center">

# Arbor

**Static analysis for Python exception and None source extraction**

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

[Installation](#installation) | [Quick Start](#quick-start) | [Documentation](#documentation) | [Contributing](#contributing)

</div>

---

## Overview

Arbor is a static analysis CLI written in Rust that extracts exceptions and None sources from Python code by traversing call graphs. It answers the question: **"What can go wrong when I call this function?"**

Python's dynamic nature makes it difficult to know:
- What exceptions a function can raise, including from its dependencies
- Where None values can originate (explicit returns, implicit returns, `dict.get()`, etc.)
- How to properly handle errors without broad `try/except` blocks
(more languages are coming soon)

Arbor solves this by parsing Python AST using tree-sitter, building a symbol index, and traversing call graphs to collect all possible failure modes.

## Features

- **Exception Extraction**: Finds all `raise` statements reachable from a function
- **None Source Detection**: Identifies explicit returns, implicit returns, and collection access patterns
- **Call Graph Traversal**: Follows function calls to configurable depth with cycle detection
- **Risk Assessment**: Categorizes functions as Low, Medium, or High risk
- **Handler Generation**: Produces grouped exception handling code suggestions
- **JSON and Markdown Output**: Machine-readable and human-readable formats
- **Virtual Environment Support**: Indexes site-packages for dependency analysis

## Installation

### From Source

```bash
git clone https://github.com/your-org/arbor.git
cd arbor
cargo build --release
```

The binary will be at `./target/release/arbor`.

### Add to PATH

```bash
# Add to ~/.zshrc or ~/.bashrc
alias arbor="/path/to/arbor/target/release/arbor"
```

## Quick Start

```bash
# Initialize Arbor in your Python project
cd /path/to/your/python/project
arbor init

# Analyze a function
arbor analyze src.api.handlers.create_user

# Query the results
arbor query function src.api.handlers.create_user

# Generate exception handler code
arbor query handle src.api.handlers.create_user
```

## Documentation

### Directory Structure

After initialization, Arbor creates:

```
project/
└── .arbor/
    ├── database.json    # Symbol index and analysis results
    ├── config.toml      # Configuration file
    └── commands/
        └── arbor.md     # AI agent documentation, move this file to .claude/commands for example
```

### Commands

#### Database Management

| Command | Description |
|---------|-------------|
| `arbor init` | Initialize `.arbor/` directory |
| `arbor init --force` | Overwrite existing database |
| `arbor init --skip-site-packages` | Skip venv indexing for faster init |
| `arbor refresh` | Re-index all symbols |
| `arbor refresh <func>` | Mark function for re-analysis |
| `arbor remove` | Delete entire `.arbor/` directory |
| `arbor remove <func>` | Remove specific function analysis |
| `arbor export -o file --format json\|markdown` | Export database |

#### Analysis

```bash
# Single function
arbor analyze src.module.function

# Class method
arbor analyze src.module.ClassName.method

# Multiple functions
arbor analyze func1 func2 func3

# All public functions in module
arbor analyze --all-public src.module

# Control traversal depth
arbor analyze src.module.function --max-depth 100

# JSON output
arbor analyze src.module.function --format json
```

#### Queries

```bash
# Overview
arbor query stats                  # Database statistics
arbor query list                   # All analyzed functions
arbor query search <keyword>       # Find functions by name

# Function details
arbor query function <name>        # Complete analysis
arbor query exceptions <name>      # Exceptions only
arbor query none <name>            # None sources only
arbor query risk <name>            # Risk level
arbor query signature <name>       # Signature and location
arbor query handle <name>          # Handler code

# Exception details
arbor query has <func> <exc>       # Check if function raises exception
arbor query chain <func> <exc>     # Call chain for exception
arbor query exception <type>       # Functions raising this type

# Call graph
arbor query callers <func>         # Functions calling this
arbor query callees <func>         # Functions called by this

# Grouping
arbor query groups                 # Exception grouping suggestions
arbor query package <name>         # Exceptions from package
```

### Configuration

`.arbor/config.toml`:

```toml
[database]
path = ".arbor/database.json"
auto_save = true

[analysis]
max_depth = 50
include_stdlib = false
timeout_seconds = 300

[environment]
python_path = ["."]
# venv_path = ".venv"

[ignore]
packages = ["tests", "__pycache__", ".git"]
functions = []
```

### Risk Levels

| Level | Criteria |
|-------|----------|
| Low | 0-1 exceptions, few None sources |
| Medium | 2-4 exceptions or multiple None sources |
| High | 5+ exceptions or complex failure modes |

### None Source Kinds

| Kind | Description |
|------|-------------|
| `explicit return` | `return None` statement |
| `implicit return` | `return` without value or function ends |
| `collection access` | `dict.get()`, `getattr()`, etc. |
| `function call` | Call to function that can return None |

## Limitations

1. **No type inference**: `obj.method()` where `obj` type is unknown cannot be followed
2. **No dynamic analysis**: `getattr(obj, name)()`, `eval()`, metaclass magic not traced
3. **External libraries**: Only analyzed if in indexed site-packages
4. **Async**: `await` calls traced, but no async-specific exception analysis
5. **Decorators**: May affect function resolution for heavily decorated code
6. **Generators**: `yield` not specially handled for exception propagation

## Troubleshooting

### "Function not found"

The function is not in the symbol index:
1. Check qualified name with `arbor query search <keyword>`
2. Verify file is in project with `arbor query stats`
3. Try with and without `src.` prefix

### Low call depth

Calls are not being resolved:
1. Calls on instance variables may not resolve
2. External library calls may not be in site-packages
3. Dynamic calls cannot be traced

### Missing library exceptions

Run `arbor init` without `--skip-site-packages` to index venv.

## Contributing

Contributions are welcome. Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Submit a pull request

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

---

<div align="center">
Built with Rust and tree-sitter
</div>
