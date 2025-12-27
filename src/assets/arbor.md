Analyze Python code for exceptions and None sources using Arbor static analysis.

User request: $ARGUMENTS

---

# Arbor Complete Reference

Arbor is a static analysis CLI that extracts exceptions and None sources from Python code by traversing call graphs. It answers: "What can go wrong when I call this function?"

## Problem Domain

Python's dynamic nature makes it hard to know:
1. What exceptions a function can raise (including from dependencies)
2. Where None values can originate (explicit returns, implicit returns, dict.get(), etc.)
3. How to properly handle errors without try/except catching everything

Arbor solves this by parsing Python AST, building a symbol index, and traversing call graphs to collect all possible failure modes.

## Architecture

```
arbor init          → Creates .arbor/ directory with database and config
arbor analyze       → Traverses call graph, extracts raises/None, stores results
arbor query         → Retrieves and formats stored analysis
arbor export        → Dumps entire database to JSON/Markdown
```

**Directory structure:**
```
project/
└── .arbor/
    ├── database.json    # Symbol index and analysis results
    └── config.toml      # Configuration file
```

The database (`.arbor/database.json`) stores:
- Symbol index: Maps qualified names to file:line locations
- Function analyses: Exceptions, None sources, call chains, risk levels
- Grouping suggestions: Which exceptions to catch together

---

## Command Reference

### Database Management

#### `arbor init [--force] [--skip-site-packages]`

Initialize `.arbor/` directory with database and config. Must run before any analysis.

```bash
arbor init                      # Create .arbor/, index all Python files
arbor init --force              # Overwrite existing database
arbor init --skip-site-packages # Faster init, skip venv packages
```

Creates:
- `.arbor/database.json` - Symbol index and analysis storage
- `.arbor/config.toml` - Configuration file (if doesn't exist)

Output shows Python version, venv path, site-packages locations, and symbol count.

#### `arbor refresh [functions...]`

Re-analyze functions after code changes.

```bash
arbor refresh                           # Refresh all analyzed functions
arbor refresh src.module.func           # Refresh specific function
arbor refresh func1 func2 func3         # Refresh multiple
```

#### `arbor remove [functions...]`

Remove analysis data.

```bash
arbor remove                            # Delete entire .arbor/ directory
arbor remove src.module.func            # Remove one function's analysis
arbor remove func1 func2                # Remove multiple
```

#### `arbor export -o <file> --format <json|markdown>`

Export all analysis data.

```bash
arbor export -o analysis.json --format json
arbor export -o analysis.md --format markdown
```

---

### Analysis

#### `arbor analyze <functions...> [options]`

Analyze functions by traversing their call graphs.

**Arguments:**
- `functions`: One or more qualified function names

**Options:**
- `--max-depth N` / `-d N`: How deep to traverse calls (default: 50, 0 = unlimited)
- `--format <markdown|json>` / `-f`: Output format
- `--venv <path>`: Explicit venv path for site-packages resolution
- `--all-public <module>`: Analyze all public functions in a module
- `--from-file <path>`: Read function names from file (one per line)

**Function Name Format:**

```
module.function                    # Top-level function
module.submodule.function          # Nested module
module.ClassName                   # Class (analyzes __init__)
module.ClassName.method            # Instance method
module.ClassName.__init__          # Constructor explicitly
src.package.module.Class.method    # Full path from project root
```

**Examples:**

```bash
# Single function
arbor analyze src.api.handlers.create_user

# Class method
arbor analyze src.websocket.handler.WebSocketHandler.handle_message

# Multiple functions
arbor analyze src.main.startup src.main.shutdown src.main.lifespan

# Deep analysis (follow all calls)
arbor analyze src.core.engine.process --max-depth 100

# Shallow analysis (direct raises only)
arbor analyze src.core.engine.process --max-depth 1

# All public functions in module
arbor analyze --all-public src.api.endpoints

# From file
echo "src.api.auth.login" > functions.txt
echo "src.api.auth.logout" >> functions.txt
arbor analyze --from-file functions.txt

# JSON output
arbor analyze src.main.run --format json

# With explicit venv
arbor analyze src.main.run --venv /path/to/.venv
```

**Output includes:**
- Risk level (🟢 Low / 🟡 Medium / 🔴 High)
- File location and line number
- Functions traced count
- Max call depth reached
- Exceptions table: type, location, definition, condition
- None sources table: kind, location, condition
- Grouping suggestions with handler code examples

---

### Queries

All query commands support `-f json` for machine-readable output.

#### Database Overview

```bash
arbor query stats                  # Summary: functions, exceptions, None sources, risk breakdown
arbor query list                   # All analyzed functions with risk levels
arbor query search <keyword>       # Find functions by name pattern
```

#### Single Function Queries

```bash
arbor query function <name>        # Complete analysis (exceptions + None + metadata)
arbor query exceptions <name>      # Just exceptions with locations and conditions
arbor query none <name>            # Just None sources with types and locations
arbor query risk <name>            # Risk level with reasoning
arbor query signature <name>       # Function signature and file location
arbor query handle <name>          # Generate try/except handler code
```

#### Exception Details

```bash
arbor query has <func> <exc>       # Check if function can raise specific exception
arbor query one-exception <func> <type>  # Details about one exception type
arbor query chain <func> <exc>     # Call chain showing how exception propagates
arbor query exception <type>       # All functions that raise this exception type
```

#### None Source Details

```bash
arbor query one-none <func> <index>  # Details about specific None source by index
```

#### Call Graph

```bash
arbor query callers <func>         # Functions that call this function
arbor query callees <func>         # Functions called by this function
```

#### Grouping & Packages

```bash
arbor query groups [package]       # Exception grouping suggestions with handler code
arbor query package <name>         # All exceptions from a package (e.g., httpx, requests)
```

#### Reference

```bash
arbor query quickref               # Quick reference for AI agents
arbor query ref                    # Alias for quickref
```

---

### Configuration

#### `arbor config init [--force]`

Generate `.arbor/config.toml` configuration file.

```bash
arbor config init                  # Create .arbor/config.toml with defaults
arbor config init --force          # Overwrite existing
```

#### `arbor config show`

Display current configuration (merged defaults + config.toml).

#### `arbor config path`

Show path to active config file.

**Configuration Options (.arbor/config.toml):**

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

---

## Output Interpretation

### Risk Levels

| Level | Icon | Criteria |
|-------|------|----------|
| Low | 🟢 | 0-1 exceptions, few None sources |
| Medium | 🟡 | 2-4 exceptions or multiple None sources |
| High | 🔴 | 5+ exceptions or complex failure modes |

### Exception Information

Each exception entry contains:
- **Type**: Exception class name (e.g., `ValueError`, `httpx.HTTPStatusError`)
- **Raise Location**: File and line where `raise` statement appears
- **Definition**: Where exception class is defined (or `(builtin)` for stdlib)
- **Condition**: Guard condition if raise is inside `if` block (e.g., `x < 0`)
- **Qualified Type**: Full module path (e.g., `requests.exceptions.ConnectionError`)

### None Source Kinds

| Kind | Description | Example |
|------|-------------|---------|
| `explicit return` | `return None` statement | `return None` |
| `implicit return` | `return` without value or function ends | `return` or no return |
| `collection access` | Methods that return None on missing key | `dict.get("key")`, `getattr(obj, "x")` |
| `function call` | Call to function that can return None | `result = maybe_none()` |

### Call Depth

- **Depth 1**: Only direct code in the function (no call following)
- **Depth N**: Followed N levels of function calls
- Higher depth = more complete analysis but slower
- Cycles are detected and not re-traversed

### Grouping Suggestions

Arbor groups exceptions by recovery strategy:
- **Retry**: Transient failures (timeouts, rate limits) → exponential backoff
- **Abort**: Unrecoverable errors → fail fast, log, alert
- **Fix Input**: Validation errors → return 400, show error to user
- **Fallback**: Optional features → use default, degrade gracefully

---

## Workflows

### Workflow: Pre-Implementation Error Handling

Before writing code that calls external functions:

```bash
cd /path/to/project
arbor init
arbor analyze the.function.i.will.call --max-depth 50
arbor query handle the.function.i.will.call
```

Use the generated handler as a starting point.

### Workflow: Audit Existing Code

Find all failure modes in a module:

```bash
arbor init
arbor analyze --all-public src.api.endpoints
arbor query list                   # See risk levels
arbor query stats                  # Overview
arbor query groups                 # How to handle them
```

### Workflow: Debug "Where Does None Come From?"

```bash
arbor init
arbor analyze src.module.problematic_function --max-depth 50
arbor query none src.module.problematic_function
```

Check each None source location to find the culprit.

### Workflow: Refactoring Safety Check

Before changing a function's error behavior:

```bash
arbor init
arbor analyze src.module.function_to_change --max-depth 50
arbor query function src.module.function_to_change
arbor query callers src.module.function_to_change
```

Document current exceptions/None sources. After refactoring, re-run and compare.

### Workflow: Dependency Risk Assessment

What can go wrong when using a library?

```bash
arbor init
arbor analyze my.code.that.uses.httpx --max-depth 50
arbor query package httpx          # All httpx exceptions
arbor query groups                 # How to handle them
```

### Workflow: Find High-Risk Functions

```bash
arbor init
arbor analyze --all-public src
arbor query list                   # Sorted by package, shows risk
arbor query -f json list | jq '.[] | select(.risk == "high")'
```

### Workflow: Generate Comprehensive Error Docs

```bash
arbor init
arbor analyze --all-public src.api
arbor export -o api_errors.md --format markdown
```

---

## JSON Output Schema

### `arbor query -f json stats`

```json
{
  "functions_analyzed": 42,
  "symbols_indexed": 19071,
  "unique_exceptions": 15,
  "unique_none_sources": 89,
  "packages_covered": 5,
  "risk_breakdown": {"high": 3, "medium": 12, "low": 27}
}
```

### `arbor query -f json function <name>`

```json
{
  "name": "src.api.auth.login",
  "location": {"file": "/path/to/auth.py", "line": 42},
  "risk": "medium",
  "functions_traced": 23,
  "call_depth": 4,
  "exceptions": [
    {
      "type": "ValueError",
      "qualified_type": "builtins.ValueError",
      "location": {"file": "auth.py", "line": 55},
      "condition": "not username",
      "message": "Username required"
    }
  ],
  "none_sources": [
    {
      "kind": "explicit_return",
      "location": {"file": "auth.py", "line": 60},
      "condition": "user is None"
    }
  ]
}
```

### `arbor query -f json groups`

```json
{
  "groups": [
    {
      "name": "Retry exceptions",
      "exceptions": ["LLMTimeoutError", "LLMRateLimitError"],
      "strategy": "retry",
      "handler_code": "for attempt in range(max_retries):..."
    }
  ]
}
```

---

## Limitations

1. **No type inference**: `obj.method()` where `obj` type is unknown won't be followed
2. **No dynamic analysis**: `getattr(obj, name)()`, `eval()`, metaclass magic not traced
3. **External libraries**: Only analyzed if in indexed site-packages
4. **Async**: `await` calls traced, but no async-specific exception analysis
5. **Decorators**: May affect function resolution for heavily decorated code
6. **Generators**: `yield` not specially handled for exception propagation

---

## Troubleshooting

### "Function not found" or "unknown:0"

The function isn't in the symbol index. Check:
1. Is the qualified name correct? Use `arbor query search <keyword>`
2. Is the file in the project? Check `arbor query stats` for indexed count
3. Try both with and without `src.` prefix

### Low call depth despite high --max-depth

Calls aren't being resolved. Common causes:
1. Calls on instance variables: `self.client.get()` where client type unknown
2. External library calls not in site-packages
3. Dynamic calls: `getattr()`, factories, etc.

### Missing exceptions from library

Run `arbor init` without `--skip-site-packages` to index venv.

### Slow analysis

- Reduce `--max-depth` for faster but less complete analysis
- Use `--skip-site-packages` on init for faster indexing
- Analyze specific functions instead of `--all-public`

---

## Execution Instructions for AI Agent

1. **Determine user intent** from $ARGUMENTS:
   - Finding exceptions? → analyze + query exceptions
   - Finding None sources? → analyze + query none
   - Error handling help? → analyze + query handle + query groups
   - Risk assessment? → analyze --all-public + query list + query stats
   - Specific function audit? → analyze + query function

2. **Locate project root** (directory with Python code, usually has src/ or main .py files)

3. **Initialize if needed**:
   ```bash
   ls .arbor/database.json || arbor init
   ```

4. **Convert user's target to qualified name**:
   - File path `src/api/auth.py` function `login` → `src.api.auth.login`
   - Class method → `module.ClassName.method_name`
   - If unsure: `arbor query search "keyword"`

5. **Run analysis**:
   ```bash
   arbor analyze <qualified_name> --max-depth 50
   ```

6. **Query based on user need**:
   ```bash
   arbor query function <name>      # Full analysis
   arbor query exceptions <name>    # Just exceptions
   arbor query none <name>          # Just None sources
   arbor query handle <name>        # Handler code
   arbor query groups               # Grouping suggestions
   ```

7. **Present results** with:
   - Summary of risk level
   - List of exceptions with locations and conditions
   - List of None sources with types
   - Recommended error handling pattern if applicable
   - Note any limitations (low call depth, unresolved calls)
