One Hundred Thousand Lines of Rust
Sep 5, 2021
In 2021, I wrote a series of posts about lessons learned maintaining medium-sized Rust projects. Here’s the list, in chronological order:

ARCHITECTURE.md
Delete Cargo Integration Tests
How to Test
Inline In Rust
Large Rust Workspaces
Fast Rust Builds

---

ARCHITECTURE.md
Feb 6, 2021
If you maintain an open-source project in the range of 10k-200k lines of code, I strongly encourage you to add an ARCHITECTURE document next to README and CONTRIBUTING. Before going into the details of why and how, I want to emphasize that this is not another “docs are good, write more docs” advice. I am pretty sloppy about documentation, and, e.g., I often use just “simplify” as a commit message. Nonetheless, I feel strongly about the issue, even to the point of pestering you :-)

I have experience with both contributing to and maintaining open-source projects. One of the lessons I’ve learned is that the biggest difference between an occasional contributor and a core developer lies in the knowledge about the physical architecture of the project. Roughly, it takes 2x more time to write a patch if you are unfamiliar with the project, but it takes 10x more time to figure out where you should change the code. This difference might be hard to perceive if you’ve been working with the project for a while. If I am new to a code base, I read each file as a sequence of logical chunks specified in some pseudo-random order. If I’ve made significant contributions before, the perception is quite different. I have a mental map of the code in my head, so I no longer read sequentially. Instead, I just jump to where the thing should be, and, if it is not there, I move it. One’s mental map is the source of truth.

I find the ARCHITECTURE file to be a low-effort high-leverage way to bridge this gap. As the name suggests, this file should describe the high-level architecture of the project. Keep it short: every recurring contributor will have to read it. Additionally, the shorter it is, the less likely it will be invalidated by some future change. This is the main rule of thumb for ARCHITECTURE — only specify things that are unlikely to frequently change. Don’t try to keep it synchronized with code. Instead, revisit it a couple of times a year.

Start with a bird’s eye overview of the problem being solved. Then, specify a more-or-less detailed codemap. Describe coarse-grained modules and how they relate to each other. The codemap should answer “where’s the thing that does X?”. It should also answer “what does the thing that I am looking at do?”. Avoid going into details of how each module works, pull this into separate documents or (better) inline documentation. A codemap is a map of a country, not an atlas of maps of its states. Use this as a chance to reflect on the project structure. Are the things you want to put near each other in the codemap adjacent when you run tree .?

Do name important files, modules, and types. Do not directly link them (links go stale). Instead, encourage the reader to use symbol search to find the mentioned entities by name. This doesn’t require maintenance and will help to discover related, similarly named things.

Explicitly call-out architectural invariants. Often, important invariants are expressed as an absence of something, and it’s pretty hard to divine that from reading the code. Think about a common example from web development: nothing in the model layer specifically doesn’t depend on the views.

Point out boundaries between layers and systems as well. A boundary implicitly contains information about the implementation of the system behind it. It even constrains all possible implementations. But finding a boundary by just randomly looking at the code is hard — good boundaries have measure zero.

After finishing the codemap, add a separate section on cross-cutting concerns.

A good example of ARCHITECTURE document is this one from rust-analyzer: architecture.md.

EXAMPLE:

# Architecture

This document describes the high-level architecture of rust-analyzer.
If you want to familiarize yourself with the code base, you are just in the right place!

See also the [guide](./guide.md), which walks through a particular snapshot of rust-analyzer code base.

Yet another resource is this playlist with videos about various parts of the analyzer:

https://www.youtube.com/playlist?list=PL85XCvVPmGQho7MZkdW-wtPtuJcFpzycE

Note that the guide and videos are pretty dated, this document should be, in general, fresher.

See also these implementation-related blog posts:

* https://rust-analyzer.github.io/blog/2019/11/13/find-usages.html
* https://rust-analyzer.github.io/blog/2020/07/20/three-architectures-for-responsive-ide.html
* https://rust-analyzer.github.io/blog/2020/09/16/challeging-LR-parsing.html
* https://rust-analyzer.github.io/blog/2020/09/28/how-to-make-a-light-bulb.html
* https://rust-analyzer.github.io/blog/2020/10/24/introducing-ungrammar.html

## Bird's Eye View

![](https://user-images.githubusercontent.com/1711539/50114578-e8a34280-0255-11e9-902c-7cfc70747966.png)

On the highest level, rust-analyzer is a thing which accepts input source code from the client and produces a structured semantic model of the code.

More specifically, input data consists of a set of test files (`(PathBuf, String)` pairs) and information about project structure, captured in the so called `CrateGraph`.
The crate graph specifies which files are crate roots, which cfg flags are specified for each crate and what dependencies exist between the crates.
This is the input (ground) state.
The analyzer keeps all this input data in memory and never does any IO.
Because the input data is source code, which typically measures in tens of megabytes at most, keeping everything in memory is OK.

A "structured semantic model" is basically an object-oriented representation of modules, functions and types which appear in the source code.
This representation is fully "resolved": all expressions have types, all references are bound to declarations, etc.
This is derived state.

The client can submit a small delta of input data (typically, a change to a single file) and get a fresh code model which accounts for changes.

The underlying engine makes sure that model is computed lazily (on-demand) and can be quickly updated for small modifications.


## Code Map

This section talks briefly about various important directories and data structures.
Pay attention to the **Architecture Invariant** sections.
They often talk about things which are deliberately absent in the source code.

Note also which crates are **API Boundaries**.
Remember, [rules at the boundary are different](https://www.tedinski.com/2018/02/06/system-boundaries.html).

### `xtask`

This is rust-analyzer's "build system".
We use cargo to compile rust code, but there are also various other tasks, like release management or local installation.
They are handled by Rust code in the xtask directory.

### `editors/code`

VS Code plugin.

### `libs/`

rust-analyzer independent libraries which we publish to crates.io.
It's not heavily utilized at the moment.

### `crates/parser`

It is a hand-written recursive descent parser, which produces a sequence of events like "start node X", "finish node Y".
It works similarly to
[kotlin's parser](https://github.com/JetBrains/kotlin/blob/4d951de616b20feca92f3e9cc9679b2de9e65195/compiler/frontend/src/org/jetbrains/kotlin/parsing/KotlinParsing.java),
which is a good source of inspiration for dealing with syntax errors and incomplete input.
Original [libsyntax parser](https://github.com/rust-lang/rust/blob/6b99adeb11313197f409b4f7c4083c2ceca8a4fe/src/libsyntax/parse/parser.rs) is what we use for the definition of the Rust language.
`TreeSink` and `TokenSource` traits bridge the tree-agnostic parser from `grammar` with `rowan` trees.

**Architecture Invariant:** the parser is independent of the particular tree structure and particular representation of the tokens.
It transforms one flat stream of events into another flat stream of events.
Token independence allows us to pares out both text-based source code and `tt`-based macro input.
Tree independence allows us to more easily vary the syntax tree implementation.
It should also unlock efficient light-parsing approaches.
For example, you can extract the set of names defined in a file (for typo correction) without building a syntax tree.

**Architecture Invariant:** parsing never fails, the parser produces `(T, Vec<Error>)` rather than `Result<T, Error>`.

### `crates/syntax`

Rust syntax tree structure and parser.
See [RFC](https://github.com/rust-lang/rfcs/pull/2256) and [./syntax.md](./syntax.md) for some design notes.

- [rowan](https://github.com/rust-analyzer/rowan) library is used for constructing syntax trees.
- `ast` provides a type safe API on top of the raw `rowan` tree.
- `ungrammar` description of the grammar, which is used to generate `syntax_kinds` and `ast` modules, using `cargo xtask codegen` command.

Tests for ra_syntax are mostly data-driven.
`test_data/parser` contains subdirectories with a bunch of `.rs` (test vectors) and `.txt` files with corresponding syntax trees.
During testing, we check `.rs` against `.txt`.
If the `.txt` file is missing, it is created (this is how you update tests).
Additionally, running `cargo xtask codegen` will walk the grammar module and collect all `// test test_name` comments into files inside `test_data/parser/inline` directory.

To update test data, run with `UPDATE_EXPECT` variable:

```bash
env UPDATE_EXPECT=1 cargo qt
```

After adding a new inline test you need to run `cargo xtest codegen` and also update the test data as described above.

Note  [`api_walkthrough`](https://github.com/rust-analyzer/rust-analyzer/blob/2fb6af89eb794f775de60b82afe56b6f986c2a40/crates/ra_syntax/src/lib.rs#L190-L348)
in particular: it shows off various methods of working with syntax tree.

See [#93](https://github.com/rust-analyzer/rust-analyzer/pull/93) for an example PR which fixes a bug in the grammar.

**Architecture Invariant:** `syntax` crate is completely independent from the rest of rust-analyzer. It knows nothing about salsa or LSP.
This is important because it is possible to make useful tooling using only the syntax tree.
Without semantic information, you don't need to be able to _build_ code, which makes the tooling more robust.
See also https://web.stanford.edu/~mlfbrown/paper.pdf.
You can view the `syntax` crate as an entry point to rust-analyzer.
`syntax` crate is an **API Boundary**.

**Architecture Invariant:** syntax tree is a value type.
The tree is fully determined by the contents of its syntax nodes, it doesn't need global context (like an interner) and doesn't store semantic info.
Using the tree as a store for semantic info is convenient in traditional compilers, but doesn't work nicely in the IDE.
Specifically, assists and refactors require transforming syntax trees, and that becomes awkward if you need to do something with the semantic info.

**Architecture Invariant:** syntax tree is built for a single file.
This is to enable parallel parsing of all files.

**Architecture Invariant:**  Syntax trees are by design incomplete and do not enforce well-formedness.
If an AST method returns an `Option`, it *can* be `None` at runtime, even if this is forbidden by the grammar.

### `crates/base_db`

We use the [salsa](https://github.com/salsa-rs/salsa) crate for incremental and on-demand computation.
Roughly, you can think of salsa as a key-value store, but it can also compute derived values using specified functions. The `base_db` crate provides basic infrastructure for interacting with salsa.
Crucially, it defines most of the "input" queries: facts supplied by the client of the analyzer.
Reading the docs of the `base_db::input` module should be useful: everything else is strictly derived from those inputs.

**Architecture Invariant:** particularities of the build system are *not* the part of the ground state.
In particular, `base_db` knows nothing about cargo.
The `CrateGraph` structure is used to represent the dependencies between the crates abstractly.

**Architecture Invariant:** `base_db` doesn't know about file system and file paths.
Files are represented with opaque `FileId`, there's no operation to get an `std::path::Path` out of the `FileId`.

### `crates/hir_expand`, `crates/hir_def`, `crates/hir_ty`

These crates are the *brain* of rust-analyzer.
This is the compiler part of the IDE.

`hir_xxx` crates have a strong ECS flavor, in that they work with raw ids and directly query the database.
There's little abstraction here.
These crates integrate deeply with salsa and chalk.

Name resolution, macro expansion and type inference all happen here.
These crates also define various intermediate representations of the core.

`ItemTree` condenses a single `SyntaxTree` into a "summary" data structure, which is stable over modifications to function bodies.

`DefMap` contains the module tree of a crate and stores module scopes.

`Body` stores information about expressions.

**Architecture Invariant:** these crates are not, and will never be, an api boundary.

**Architecture Invariant:** these crates explicitly care about being incremental.
The core invariant we maintain is "typing inside a function's body never invalidates global derived data".
i.e., if you change the body of `foo`, all facts about `bar` should remain intact.

**Architecture Invariant:** hir exists only in context of particular crate instance with specific CFG flags.
The same syntax may produce several instances of HIR if the crate participates in the crate graph more than once.

### `crates/hir`

The top-level `hir` crate is an **API Boundary**.
If you think about "using rust-analyzer as a library", `hir` crate is most likely the façade you'll be talking to.

It wraps ECS-style internal API into a more OO-flavored API (with an extra `db` argument for each call).

**Architecture Invariant:** `hir` provides a static, fully resolved view of the code.
While internal `hir_*` crates _compute_ things, `hir`, from the outside, looks like an inert data structure.

`hir` also handles the delicate task of going from syntax to the corresponding `hir`.
Remember that the mapping here is one-to-many.
See `Semantics` type and `source_to_def` module.

Note in particular a curious recursive structure in `source_to_def`.
We first resolve the parent _syntax_ node to the parent _hir_ element.
Then we ask the _hir_ parent what _syntax_ children does it have.
Then we look for our node in the set of children.

This is the heart of many IDE features, like goto definition, which start with figuring out the hir node at the cursor.
This is some kind of (yet unnamed) uber-IDE pattern, as it is present in Roslyn and Kotlin as well.

### `crates/ide`

The `ide` crate builds on top of `hir` semantic model to provide high-level IDE features like completion or goto definition.
It is an **API Boundary**.
If you want to use IDE parts of rust-analyzer via LSP, custom flatbuffers-based protocol or just as a library in your text editor, this is the right API.

**Architecture Invariant:** `ide` crate's API is build out of POD types with public fields.
The API uses editor's terminology, it talks about offsets and string labels rather than in terms of definitions or types.
It is effectively the view in MVC and viewmodel in [MVVM](https://en.wikipedia.org/wiki/Model%E2%80%93view%E2%80%93viewmodel).
All arguments and return types are conceptually serializable.
In particular, syntax tress and and hir types are generally absent from the API (but are used heavily in the implementation).
Shout outs to LSP developers for popularizing the idea that "UI" is a good place to draw a boundary at.

`ide` is also the first crate which has the notion of change over time.
`AnalysisHost` is a state to which you can transactionally `apply_change`.
`Analysis` is an immutable snapshot of the state.

Internally, `ide` is split across several crates. `ide_assists`, `ide_completion` and `ide_ssr` implement large isolated features.
`ide_db` implements common IDE functionality (notably, reference search is implemented here).
The `ide` contains a public API/façade, as well as implementation for a plethora of smaller features.

**Architecture Invariant:** `ide` crate strives to provide a _perfect_ API.
Although at the moment it has only one consumer, the LSP server, LSP *does not* influence it's API design.
Instead, we keep in mind a hypothetical _ideal_ client -- an IDE tailored specifically for rust, every nook and cranny of which is packed with Rust-specific goodies.

### `crates/rust-analyzer`

This crate defines the `rust-analyzer` binary, so it is the **entry point**.
It implements the language server.

**Architecture Invariant:** `rust-analyzer` is the only crate that knows about LSP and JSON serialization.
If you want to expose a datastructure `X` from ide to LSP, don't make it serializable.
Instead, create a serializable counterpart in `rust-analyzer` crate and manually convert between the two.

`GlobalState` is the state of the server.
The `main_loop` defines the server event loop which accepts requests and sends responses.
Requests that modify the state or might block user's typing are handled on the main thread.
All other requests are processed in background.

**Architecture Invariant:** the server is stateless, a-la HTTP.
Sometimes state needs to be preserved between requests.
For example, "what is the `edit` for the fifth completion item of the last completion edit?".
For this, the second request should include enough info to re-create the context from scratch.
This generally means including all the parameters of the original request.

`reload` module contains the code that handles configuration and Cargo.toml changes.
This is a tricky business.

**Architecture Invariant:** `rust-analyzer` should be partially available even when the build is broken.
Reloading process should not prevent IDE features from working.

### `crates/toolchain`, `crates/project_model`, `crates/flycheck`

These crates deal with invoking `cargo` to learn about project structure and get compiler errors for the "check on save" feature.

They use `crates/path` heavily instead of `std::path`.
A single `rust-analyzer` process can serve many projects, so it is important that server's current directory does not leak.

### `crates/mbe`, `crates/tt`, `crates/proc_macro_api`, `crates/proc_macro_srv`

These crates implement macros as token tree -> token tree transforms.
They are independent from the rest of the code.

### `crates/cfg`

This crate is responsible for parsing, evaluation and general definition of `cfg` attributes.

### `crates/vfs`, `crates/vfs-notify`

These crates implement a virtual file system.
They provide consistent snapshots of the underlying file system and insulate messy OS paths.

**Architecture Invariant:** vfs doesn't assume a single unified file system.
i.e., a single rust-analyzer process can act as a remote server for two different machines, where the same `/tmp/foo.rs` path points to different files.
For this reason, all path APIs generally take some existing path as a "file system witness".

### `crates/stdx`

This crate contains various non-rust-analyzer specific utils, which could have been in std, as well
as copies of unstable std items we would like to make use of already, like `std::str::split_once`.

### `crates/profile`

This crate contains utilities for CPU and memory profiling.


## Cross-Cutting Concerns

This sections talks about the things which are everywhere and nowhere in particular.

### Code generation

Some of the components of this repository are generated through automatic processes.
`cargo xtask codegen` runs all generation tasks.
Generated code is generally committed to the git repository.
There are tests to check that the generated code is fresh.

In particular, we generate:

* API for working with syntax trees (`syntax::ast`, the [`ungrammar`](https://github.com/rust-analyzer/ungrammar) crate).
* Various sections of the manual:

    * features
    * assists
    * config

* Documentation tests for assists

**Architecture Invariant:** we avoid bootstrapping.
For codegen we need to parse Rust code.
Using rust-analyzer for that would work and would be fun, but it would also complicate the build process a lot.
For that reason, we use syn and manual string parsing.

### Cancellation

Let's say that the IDE is in the process of computing syntax highlighting, when the user types `foo`.
What should happen?
`rust-analyzer`s answer is that the highlighting process should be cancelled -- its results are now stale, and it also blocks modification of the inputs.

The salsa database maintains a global revision counter.
When applying a change, salsa bumps this counter and waits until all other threads using salsa finish.
If a thread does salsa-based computation and notices that the counter is incremented, it panics with a special value (see `Canceled::throw`).
That is, rust-analyzer requires unwinding.

`ide` is the boundary where the panic is caught and transformed into a `Result<T, Cancelled>`.

### Testing

Rust Analyzer has three interesting [system boundaries](https://www.tedinski.com/2018/04/10/making-tests-a-positive-influence-on-design.html) to concentrate tests on.

The outermost boundary is the `rust-analyzer` crate, which defines an LSP interface in terms of stdio.
We do integration testing of this component, by feeding it with a stream of LSP requests and checking responses.
These tests are known as "heavy", because they interact with Cargo and read real files from disk.
For this reason, we try to avoid writing too many tests on this boundary: in a statically typed language, it's hard to make an error in the protocol itself if messages are themselves typed.
Heavy tests are only run when `RUN_SLOW_TESTS` env var is set.

The middle, and most important, boundary is `ide`.
Unlike `rust-analyzer`, which exposes API, `ide` uses Rust API and is intended for use by various tools.
A typical test creates an `AnalysisHost`, calls some `Analysis` functions and compares the results against expectation.

The innermost and most elaborate boundary is `hir`.
It has a much richer vocabulary of types than `ide`, but the basic testing setup is the same: we create a database, run some queries, assert result.

For comparisons, we use the `expect` crate for snapshot testing.

To test various analysis corner cases and avoid forgetting about old tests, we use so-called marks.
See the `marks` module in the `test_utils` crate for more.

**Architecture Invariant:** rust-analyzer tests do not use libcore or libstd.
All required library code must be a part of the tests.
This ensures fast test execution.

**Architecture Invariant:** tests are data driven and do not test the API.
Tests which directly call various API functions are a liability, because they make refactoring the API significantly more complicated.
So most of the tests look like this:

```rust
fn check(input: &str, expect: expect_test::Expect) {
    // The single place that actually exercises a particular API
}


#[test]
fn foo() {
    check("foo", expect![["bar"]]);
}

#[test]
fn spam() {
    check("spam", expect![["eggs"]]);
}
// ...and a hundred more tests that don't care about the specific API at all.
```

To specify input data, we use a single string literal in a special format, which can describe a set of rust files.
See the `Fixture` type.

**Architecture Invariant:** all code invariants are tested by `#[test]` tests.
There's no additional checks in CI, formatting and tidy tests are run with `cargo test`.

**Architecture Invariant:** tests do not depend on any kind of external resources, they are perfectly reproducible.

### Error Handling

**Architecture Invariant:** core parts of rust-analyzer (`ide`/`hir`) don't interact with the outside world and thus can't fail.
Only parts touching LSP are allowed to do IO.

Internals of rust-analyzer need to deal with broken code, but this is not an error condition.
rust-analyzer is robust: various analysis compute `(T, Vec<Error>)` rather than `Result<T, Error>`.

rust-analyzer is a complex long-running process.
It will always have bugs and panics.
But a panic in an isolated feature should not bring down the whole process.
Each LSP-request is protected by a `catch_unwind`.
We use `always` and `never` macros instead of `assert` to gracefully recover from impossible conditions.

### Observability

rust-analyzer is a long-running process, so its important to understand what's going on inside.
We have several instruments for that.

The event loop that runs rust-analyzer is very explicit.
Rather than spawning futures or scheduling callbacks (open), the event loop accepts an `enum` of possible events (closed).
It's easy to see all the things that trigger rust-analyzer processing, together with their performance

rust-analyzer includes a simple hierarchical profiler (`hprof`).
It is enabled with `RA_PROFILE='*>50` env var (log all (`*`) actions which take more than `50` ms) and produces output like:

```
85ms - handle_completion
    68ms - import_on_the_fly
        67ms - import_assets::search_for_relative_paths
             0ms - crate_def_map:wait (804 calls)
             0ms - find_path (16 calls)
             2ms - find_similar_imports (1 calls)
             0ms - generic_params_query (334 calls)
            59ms - trait_solve_query (186 calls)
         0ms - Semantics::analyze_impl (1 calls)
         1ms - render_resolution (8 calls)
     0ms - Semantics::analyze_impl (5 calls)
```

This is cheap enough to enable in production.


Similarly, we save live object counting (`RA_COUNT=1`).
It is not cheap enough to enable in prod, and this is a bug which should be fixed.

---

Delete Cargo Integration Tests
Feb 27, 2021
Click bait title! We’ll actually look into how integration and unit tests are implemented in Cargo. A few guidelines for organizing test suites in large Cargo projects naturally arise out of these implementation differences. And, yes, one of those guidelines will turn out to be: “delete all integration tests but one”.

Keep in mind that this post is explicitly only about Cargo concepts. It doesn’t discuss relative merits of integration or unit styles of testing. I’d love to, but that’s going to be a loooong article some other day!

Loomings 🐳
When you use Cargo, you can put #[test] functions directly next to code, in files inside src/ directory. Alternatively, you can put them into dedicated files inside tests/:

```text
awesomeness-rs/
  Cargo.toml
  src/          # unit tests go here
    lib.rs
    submodule.rs
    submodule/
      tests.rs
  tests/        # integration tests go here
    is_awesome.rs
```
I stress that unit/integration terminology is based purely on the location of the #[test] functions, and not on what those functions actually do.

To build unit tests, Cargo runs

```bash
rustc --test src/lib.rs
```
Then rustc compiles the library with --cfg test. It also injects a generated fn main(), which invokes all functions annotated with #[test]. The result is an executable file which, when run subsequently by Cargo, executes the tests.

Integration tests are built differently. First, Cargo uses rustc to compile the library as usual, without --cfg test:

```bash
rustc --crate-type=rlib src/lib.rs
```
This produces an .rlib file — a compiled library.

Then, for each file in the tests directory, Cargo runs the equivalent of

```bash
rustc --test --extern awesomeness=path/to/awesomeness.rlib \
    ./tests/is_awesome.rs
```
That is, each integration test is compiled into a separate binary. Running those binaries executes the test functions.

Implications
Note that rustc needs to repeatedly re-link the library crate with each of the integration tests. This can add up to a significant compilation time blow up for tests. That is why I recommend that large projects should have only one integration test crate with several modules. That is, don’t do this:

```text
tests/
  foo.rs
  bar.rs
```
Do this instead:

```text
tests/
  integration/
    main.rs
    foo.rs
    bar.rs
```
When a refactoring along these lines was applied to Cargo itself, the effects were substantial (numbers). The time to compile the test suite decreased 3x. The size of on-disk artifacts decreased 5x.

It can’t get better than this, right? Wrong! Rust tests by default are run in parallel. The main that is generated by rustc spawns several threads to saturate all of the CPU cores. However, Cargo itself runs test binaries sequentially. This makes sense — otherwise, concurrently executing test binaries oversubscribe the CPU. But this means that multiple integration tests leave performance on the table. The critical path is the sum of longest tests in each binary. The more binaries, the longer the path. For one of my projects, consolidating several integration tests into one reduced the time to run the test suite from 20 seconds to just 13.

A nice side-effect of a single modularized integration test is that sharing the code between separate tests becomes trivial, you just pull it into a submodule. There’s no need to awkwardly repeat mod common; for each integration test.

Rules of Thumb
If the project I am working with is small, I don’t worry about test organization. There’s no need to make tests twice as fast if they are already nearly instant.

Conversely, if the project is large (a workspace with many crates) I worry about test organization a lot. Slow tests are a boiling frog kind of problem. If you do not proactively fix it, everything is fine up until the moment you realize you need to sink a week to untangle the mess.

For a library with a public API which is published to crates.io, I avoid unit tests. Instead, I use a single integration tests, called it (integration test):

```text
tests/
  it.rs
# Or, for larger crates
tests/
  it/
    main.rs
    foo.rs
    bar.rs
```
Integration tests use the library as an external crate. This forces the usage of the same public API that consumers use, resulting in a better design feedback.

For an internal library, I avoid integration tests all together. Instead, I use Cargo unit tests for “integration” bits:

```text
src/
  lib.rs
  tests.rs
  tests/
    foo.rs
    bar.rs
```
That way, I avoid linking the separate integration tests binary altogether. I also have access to non-pub API of the crate, which is often useful.

Assorted Tricks
First, documentation tests are extremely slow. Each doc test is linked as a separate binary. For this reason, avoid doc tests in internal libraries for big projects and add this to Cargo.toml:

```toml
[lib]
doctest = false
```
Second, prefer

```rust
#[cfg(test)]
mod tests; // tests in `tests.rs` file
```
to

```rust
#[cfg(test)]
mod tests {
    // tests here
}
```
This way, when you modify just the tests, Cargo is smart to not recompile the library crate. It knows that the contents of tests.rs only affects compilation when --test is passed to rustc. Learned this one from @petrochenkov, thanks!

Third, even if you stick to unit tests, the library is recompiled twice: once with, and once without --test. For this reason, folks from pernosco go even further. They add

```toml
[lib]
test = false
```
to Cargo.toml, make all APIs they want to unit test public and have a single test crate for the whole workspace. This crate links everything and contains all the unit tests.

Discussion on /r/rust.

---

How to Test
May 31, 2021
Alternative titles:
     Unit Tests are a Scam
     Test Features, Not Code
     Data Driven Integrated Tests

This post describes my current approach to testing. When I started programming professionally, I knew how to write good code, but good tests remained a mystery for a long time. This is not due to the lack of advice — on the contrary, there’s abundance of information & terminology about testing. This celestial emporium of benevolent knowledge includes TDD, BDD, unit tests, integrated tests, integration tests, end-to-end tests, functional tests, non-functional tests, blackbox tests, glassbox tests, …

Knowing all this didn’t help me to create better software. What did help was trying out different testing approaches myself, and looking at how other people write tests. Keep in mind that my background is mostly in writing compiler front-ends for IDEs. This is a rather niche area, which is especially amendable to testing. Compilers are pure self-contained functions. I don’t know how to best test modern HTTP applications built around inter-process communication.

Without further ado, let’s see what I have learned!

Further ado(2024-05-21): while writing this post, I was missing a key piece of terminology for crisply describing various kinds of tests. If you like this post, you might want to read Unit and Integration Tests . That post supplies better vocabulary for talking about phenomena described in the present article.

Test Driven Design Ossification
This is something I inflicted upon myself early in my career, and something I routinely observe. You want to refactor some code, say add a new function parameter. Turns out, there are a dozen of tests calling this function, so now a simple refactor also involves fixing all the tests.

There is a simple, mechanical fix to this problem: introduce the check function which encapsulates API under test. It’s easier to explain using a toy example. Let’s look at testing something simple, like a binary search, just to illustrate the technique.

We start with direct testing:

```rust
/// Given a *sorted* `haystack`, returns `true`
/// if it contains the `needle`.
fn binary_search(haystack: &[T], needle: &T) -> bool {
    ...
}
#[test]
fn binary_search_empty() {
  let res = binary_search(&[], &0);
  assert_eq!(res, false);
}
#[test]
fn binary_search_singleton() {
  let res = binary_search(&[92], &0);
  assert_eq!(res, false);
  let res = binary_search(&[92], &92);
  assert_eq!(res, true);
  let res = binary_search(&[92], &100);
  assert_eq!(res, false);
}
// And a dozen more of other similar tests...
```
Some time passes, and we realize that -> bool is not the best signature for binary search. It’s better if it returned an insertion point (an index where element should be inserted to maintain sortedness). That is, we want to change the signature to

```rust
fn binary_search(haystack: &[T], needle: &T) -> Result<usize, usize>;
```
Now we have to change every test, because the tests are tightly coupled to the specific API.

My solution to this problem is making the tests data driven. Instead of every test interacting with the API directly, I like to define a single check function which calls the API. This function takes a pair of input and expected result. For binary search example, it will look like this:

```rust
#[track_caller]
fn check(
  input_haystack: &[i32],
  input_needle: i32,
  expected_result: bool,
) {
  let actual_result =
    binary_search(input_haystack, &input_needle);
  assert_eq!(expected_result, actual_result);
}
#[test]
fn binary_search_empty() {
  check(&[], 0, false);
}
#[test]
fn binary_search_singleton() {
  check(&[92], 0, false);
  check(&[92], 92, true);
  check(&[92], 100, false);
}
```
Now, when the API of the binary_search function changes, we only need to adjust the single place — check function:

```rust
#[track_caller]
fn check(
  input_haystack: &[i32],
  input_needle: i32,
  expected_result: bool,
) {
  let actual_result =
    binary_search(input_haystack, &input_needle).is_ok();
  assert_eq!(expected_result, actual_result);
}
```
To be clear, after you’ve done the refactor, you’ll need to adjust the tests to check the index as well, but this can be done separately. Existing test suite does not impede changes.

Key point: keep an eye on tests standing in a way of refactors. Use the check idiom to make tests resilient to changes.

Keep in mind that the binary search example is artificially simple. The main danger here is that this is a boiling frog type of situation. While the project is small and the tests are few, you don’t notice that refactors are ever so slightly longer than necessary. Then, several tens of thousands lines of code later, you realize that to make a simple change you need to fix a hundred tests.

Test Friction
Almost no one likes to write tests. I’ve noticed many times how, upon fixing a trivial bug, I am prone to skipping the testing work. Specifically, if writing a test is more effort than the fix itself, testing tends to go out of the window. Hence,

Key point: work hard on making adding new tests trivial.

Coming back to the binary search example, note how check function reduces the amount of typing to add a new test. For tests, this is a significant saving, not because typing is hard, but because it lowers the cognitive barrier to actually do the work.

Test Features, Not Code
The over-simplified binary search example can be stretched further. What if you replace the sorted array with a hash map inside your application? Or what if the calling code no longer needs to search at all, and wants to process all of the elements instead?

Good code is easy to delete. Tests represent an investment into existing code, and make it costlier to delete (or change).

The solution is to write tests for features in such a way that they are independent of the code. I like to use the neural network test for this:

Neural Network Test
Can you re-use the test suite if your entire software is replaced with an opaque neural network?

To give a real-life example this time, suppose that you are writing that part of code-completion engine which sorts potential completions according to relevance. (something I should probably be doing right now, instead of writing this article :-) )

Internally, you have a bunch of functions that compute relevance facts, like:

Is there direct type match (.foo has the desired type)?
Is there an indirect type match (.foo.bar has the right type)?
How frequently is this completion used in the current module?
Then, there’s the final ranking function that takes these facts and comes up with an overall rank.

The classical unit-test approach here would be to write a bunch of isolated tests for each of the relevance functions, and a separate bunch of tests which feeds the ranking function a list of relevance facts and checks the final score.

This approach obviously fails the neural network test.

An alternative approach is to write a test to check that at a given position a specific ordered list of entries is returned. That suite could work as a cross-validation for an ML-based implementation.

In practice, it’s unlikely (but not impossible), that we use actual ML here. But it’s highly probably that the naive independent weights model isn’t the end of the story. At some point there will be special cases which would necessitate change of the interface.

Key point: duh, test features, not code! Test at the boundaries.

If you build a library, the boundary is the public API. If you are building an application, you are not building the library. The boundary is what a human in front of a display sees.

Note that this advice goes directly against one common understanding of unit-testing. I am fairly confident that it results in better software over the long run.

Make Tests Fast
There’s one talk about software engineering, which stands out for me, and which is my favorite. It is Boundaries by Gary Bernhardt. There’s a point there though, which I strongly disagree with:

Integration Tests are Superlinear?
When you use integration tests, any new feature is accompanied by a bit of new code and a new test. However, new code slows down all other tests, so the the overall test suite becomes slow, as the total time grows super-linearly.

I don’t think more code under test translates to slower test suite. Merge sort spends more lines of code than bubble sort, but it is way faster.

In the abstract, yes, more code generally means more execution time, but I doubt this is the defining factor in tests execution time. What actually happens is usually:

Input/Output — reading just a bit from a disk, network or another process slows down the tests significantly.
Outliers — very often, testing time is dominated by only a couple of slow tests.
Overly large input — throwing enough data at any software makes it slow.
The problem with integrated tests is not code volume per se, but the fact that they typically mean doing a lot of IO. But this doesn’t need to be the case

Key point: architecture the software to keep as much as possible sans io. Let the caller do input and output, and let the callee do compute. It doesn’t matter if the callee is large and complex. Even if it is the whole compiler, testing is fast and easy as long as no IO is involved.

Nonetheless, some tests are going to be slow. It pays off to introduce the concept of slow tests early on, arrange the skipping of such tests by default and only exercise them on CI. You don’t need to be fancy, just checking an environment variable at the start of the test is perfectly fine:

```rust
#[test]
fn completion_works_with_real_standard_library() {
  if std::env::var("RUN_SLOW_TESTS").is_err() {
    return;
  }
  ...
}
```
Definitely do not use conditional compilation to hide slow tests — it’s an obvious solution which makes your life harder (similar observation from the Go ecosystem).

To deal with outliers, print each test’s execution time by default. Having the numbers fly by gives you immediate feedback and incentive to improve.

Data Driven Testing
All these together lead to a particular style of architecture and tests, which I call data driven testing. The bulk of the software is a pure function, where the state is passed in explicitly. Removing IO from the picture necessitates that the interface of software is specified in terms of data. Value in, value out.

One property of data is that it can be serialized and deserialized. That means that the check style tests can easily accept arbitrary complex input, which is specified in a structured format (JSON), ad-hoc plain text format, or via embedded DSL (builder-style interface for data objects).

Similarly, The “expected” argument of check is data. It is a result which is more-or-less directly displayed to the user.

A convincing example of a data driven test would be a “Goto Definition” tests from rust-analyzer (source):


In this case, the check function has only a single argument — a string which specifies both the input and the expected result. The input is a rust project with three files (//- /file.rs syntax shows the boundary between the files). The current cursor position is also a part of the input and is specified with the $0 syntax. The result is the //^^^ comment which marks the target of the “Goto Definition” call. The check function creates an in-memory Rust project, invokes “Goto Definition” at the position signified by $0, and checks that the result is the position marked with ^^^.

Note that this is decidedly not a unit test. Nothing is stubbed or mocked. This test invokes the whole compilation pipeline: virtual file system, parser, macro expander, name resolution. It runs on top of our incremental computation engine. It touches a significant fraction of the IDE APIs. Yet, it takes 4ms in debug mode (and 500µs in release mode). And note that it absolutely does not depend on any internal API — if we replace our dumb compiler with sufficiently smart neural net, nothing needs to be adjusted in the tests.

There’s one question though: why on earth am I using a png image to display a bit of code? Only to show that the raw string literal (r#""#) which contains Rust code is highlighted as such. This is possible because we re-use the same input format (with //-, $0 and couple of other markup elements) for almost every test in rust-analyzer. As such, we can invest effort into building cool things on top of this format, which subsequently benefit all our tests.

Expect Tests
Previous example had a complex data input, but a relatively simple data output — a position in the file. Often, the output is messy and has a complicated structure as well (a symptom of rho problem). Worse, sometimes the output is a part that is changed frequently. This often necessitates updating a lot of tests. Going back to the binary search example, the change from -> bool to -> Result<usize, usize> was an example of this effect.

There is a technique to make such simultaneous changes to all gold outputs easy — testing with expectations. You specify the expected result as a bit of data inline with the test. There’s a special mode of running the test suite for updating this data. Instead of failing the test, a mismatch between expected and actual causes the gold value to be updated in-place. That is, the test framework edits the code of the test itself.

Here’s an example of this workflow in rust-analyzer, used for testing code completion:

Often, just Debug representation of the type works well for expect tests, but you can do something more fun. See this post from Jane Street for a great example: Using ASCII waveforms to test hardware designs.

There are several libraries for this in Rust: insta, k9, expect-test.

Fluent Assertions
An extremely popular genre for a testing library is a collection of fluent assertions:

```rust
// Built-in assertion:
assert!(x > y);
// Fluent assertion:
assert_that(x).is_greater_than(y);
```
The benefit of this style are better error messages. Instead of just “false is not true”, the testing framework can print values for x and y.

I don’t find this useful. Using the check style testing, there are very few assertions actually written in code. Usually, I start with plain asserts without messages. The first time I debug an actual test failure for a particular function, I spend some time to write a detailed assertion message. To me, fluent assertions are not an attractive point on the curve that includes plain asserts and hand-written, context aware explanations of failures. A notable exception here is pytest approach — this testing framework overrides the standard assert to provide a rich diff without ceremony.

Key Point: invest into testing infrastructure in a scalable way. Write a single check function with artisanally crafted error message, define a universal fixture format for the input, use expectation testing for output.

Peeking Inside
One apparent limitation of the style of integrated testing I am describing is checking for properties which are not part of the output. For example, if some kind of caching is involved, you might want to check that the cache is actually being hit, and is not just sitting there. But, by definition, cache is not something that an outside client can observe.

The solution to this problem is to make this extra data a part of the system’s output by adding extra observability points. A good example here is Cargo’s test suite. It is set-up in an integrated, data-driven fashion. Each tests starts with a succinct DSL for setting up a tree of files on disk. Then, a full cargo command is invoked. Finally, the test looks at the command’s output and the resulting state of the file system, and asserts the relevant facts.

Tests for caching additionally enable verbose internal logging. In this mode, Cargo prints information about cache hits and misses. These messages are then used in assertions.

A close idea is coverage marks. Some times, you want to check that something does not happen. Tests for this tend to be fragile — often the thing does not happen, but for the wrong reason. You can add a side channel which explains the reasoning behind particular behavior, and additionally assert this as well.

Externalized Tests
In the ultimate stage of data driven tests the definitions of test cases are moved out of test functions and into external files. That is, you don’t do this:

```rust
#[test]
fn test_foo() {
  check("foo", "oof")
}
#[test]
fn test_bar() {
  check("bar", "rab")
}
```
Rather, there is a single test that looks like this:

```rust
#[test]
fn test_all() {
  for file in read_dir("./test_data/in") {
    let input = read_to_string(
      &format!("./test_data/in/{}", file),
    );
    let output = read_to_string(
      &format!("./test_data/out/{}", file),
    );
    check(input, output)
  }
}
```
I have a love-hate relationship with this approach. It has at least two attractive properties. First, it forces data driven approach without any cheating. Second, it makes the test suite more re-usable. An alternative implementation in a different programming language can use the same tests.

But there’s a drawback as well — without literal #[test] attributes, integration with tooling suffers. For example, you don’t automatically get “X out of Y tests passed” at the end of test run. You can’t conveniently debug just a single test, there isn’t a helpful “Run” icon/shortcut you can use in an IDE.

When I do externalized test cases, I like to leave a trivial smoke test behind:

```rust
#[test]
fn smoke() {
  check("", "");
}
```
If I need to debug a failing external test, I first paste the input into this smoke test, and then get my IDE tooling back.

Beyond Example Based Testing
Reading from a file is not the most fun way to come up with a data input for a check function.

Here are a few other popular ones:

Property Based Testing
Generate the input at random and verify that the output makes sense. For a binary search, check that the needle indeed lies between the two elements at the insertion point.

Full Coverage
Better still, instead of generating some random inputs, just check that the answer is correct for all inputs. This is how you should be testing binary search — generate every sorted list of length at most 7 with numbers in the 0..=6 range. Then, for each list and for each number, check that the binary search gives the same result as a naive linear search.

Coverage Guided Fuzzing
Just throw random bytes at the check function. Random bytes probably don’t make much sense, but it’s good to verify that the program returns an error instead of summoning nasal demons. Instead of piling bytes completely at random, observe which branches are taken, and try to invent byte sequences which cover more branches. Note that this test is polymorphic in the system under test.

Structured Fuzzing / Coverage Guided Property Testing
Use random bytes as a seed to generate “syntactically valid” inputs, then see you software crash and burn when the most hideous edge cases are uncovered. If you use Rust, check out wasm-smith and arbitrary crates.

Key Point: once you formulated the tests in terms of data, you no longer need to write code to add your tests. If code is not required, you can generate test cases easily.

The External World
What if isolating IO is not possible, and the application is fundamentally build around interacting with external systems? In this case, my advice is to just accept that the tests are going to be slow, and might need extra effort to avoid flakiness.

Cargo is the perfect case study here. Its raison d’être is orchestrating a herd of external processes. Let’s look at the basic test:

```rust
#[test]
fn cargo_compile_simple() {
  let p = project()
    .file("Cargo.toml", &basic_bin_manifest("foo"))
    .file("src/foo.rs", &main_file(r#""i am foo""#, &[]))
    .build();
  p.cargo("build").run();
  assert!(p.bin("foo").is_file());
  p.process(&p.bin("foo")).with_stdout("i am foo\n").run();
}
```
The project() part is a builder, which describes the state of the a system. First, .build() writes the specified files to a disk in a temporary directory. Then, p.cargo("build").run() executes the real cargo build command. Finally, a bunch of assertions are made about the end state of the file system.

Neural network test: this is completely independent of internal Cargo APIs, by virtue of interacting with a cargo process via IPC.

To give an order-of-magnitude feeling for the cost of IO, Cargo’s test suite takes around seven minutes (-j 1), while rust-analyzer finishes in less than half a minute.

An interesting case is the middle ground, when the IO-ing part is just big and important enough to be annoying. That is the case for rust-analyzer — although almost all code is pure, there’s a part which interacts with a specific editor. What makes this especially finicky is that, in the case of Cargo, it’s Cargo who calls external processes. With rust-analyzer, it’s something which we don’t control, the editor, which schedules the IO. This often results in hard-to-imagine bugs which are caused by particularly weird environments.

I don’t have good answers here, but here are the tricks I use:

Accept that something will break during integration. Even if you always create perfect code and never make bugs, your upstream integration point will be buggy sometimes.
Make integration bugs less costly:
use release trains,
make patch release process non-exceptional and easy,
have a checklist for manual QA before the release.
Separate the tricky to test bits into a separate project. This allows you to write slow and not 100% reliable tests for integration parts, while keeping the core test suite fast and dependable.
Key Point: if you can’t avoid IO, embrace it. Even if a data driven test suite is slow, it gives you a lot of confidence that features work, without intervening with refactors.

The Concurrent World
Consider the following API:

```rust
fn do_stuff_in_background(p: Param) {
  std::thread::spawn(move || {
    // Stuff
  })
}
```
This API is fundamentally untestable. Can you see why? It spawns a concurrent computation, but it doesn’t allow waiting for this computation to be finished. So, any test that calls do_stuff_in_background can’t check that the “Stuff” is done. Worse, even tests which do not call this function might start to fail — they now can get interference from other tests. The concurrent computation can outlive the test that originally spawned it.

This problem plagues almost every concurrent application I see. A common symptom is adding timeouts and sleeps to test, to increase the probability of stuff getting done. Such timeouts are another common cause of slow test suites.

What makes this problem truly insidious is that there’s no work-around. Broken once, causality link is not reforgable by a layer above.

The solution is simple: don’t do this.

Key Point: grab a (large) cup of coffee and go read Go statement considered harmful. I will wait until you are done, and then proceed with my article.

Layers
Another common problem I see in complex projects is a beautifully layered architecture, which is “inverted” in tests.

Let’s say you have something fabulous, like L1 <- L2 <- L3 <- L4. To test L1, the path of least resistance is often to write tests which exercise L4. You might even think that this is the setup I am advocating for. Not exactly.

The problem with L1 <- L2 <- L3 <- L4 <- Tests is that working on L1 becomes slower, especially in compiled languages. If you make a change to L1, then, before you get to the tests, you need to recompile the whole chain of reverse dependencies. My “favorite” example here is rustc — when I worked on the lexer (T1), I spent a lot of time waiting for the rest of the compiler to be rebuild to check my small change.

The right setup here is to write integrated tests for each layer:

```text
L1 <- Tests
L1 <- L2 <- Tests
L1 <- L2 <- L3 <- Tests
L1 <- L2 <- L3 <- L4 <- Tests
```
Note that testing L4 involves testing L1, L2 an L3. This is not a problem. Due to layering, only L4 needs to be recompiled. Other layers don’t affect run time meaningfully. Remember — it’s IO (and sleep-based synchronization) that kills performance, not just code volume.

Test Everything
In a nutshell, a #[test] is just a bit of code which is plugged into the build system to be executed automatically. Use this to your advantage, simplify the automation by moving as much as possible into tests.

Here’s some things in rust-analyzer which are just tests:

Code formatting (most common one — you don’t need an extra pile of YAML in CI, you can shell out to the formatter from the test).
Checking that the history does not contain merge commits and teaching new contributors git survival skills.
Collecting the manual from specially-formatted doc comments across the code base.
Checking that the code base is, in fact, reasonably well-documented.
Ensuring that the licenses of dependencies are compatible.
Ensuring that high-level operations are linear in the size of the input. Syntax-highlight a synthetic file of 1, 2, 4, 8, 16 kilobytes, run linear regression, check that result looks like a line rather than a parabola.
Use Bors
This essay already mentioned a couple of cognitive tricks for better testing: reducing the fixed costs for adding new tests, and plotting/printing test times. The best trick in a similar vein is the “not rocket science” rule of software engineering.

The idea is to have a robot which checks that the merge commit passes all the tests, before advancing the state of the main branch.

Besides the evergreen master, such bot adds pressure to keep the test suite fast and non-flaky. This is another boiling frog, something you need to constantly keep an eye on. If you have any a single flaky test, it’s very easy to miss when the second one is added.

Key point: use https://bors.tech, a no-nonsense implementation of “not rocket science” rule.

Recap
This was a long essay. Let’s look back at some of the key points:

There is a lot of information about testing, but it is not always helpful. At least, it was not helpful for me.
The core characteristic of the test suite is how easy it makes changing the software under test.
To that end, a good strategy is to focus on testing the features of the application, rather than on testing the code used to implement those features.
A good test suite passes the neural network test — it is still useful if the entire application is replaced by an ML model which just comes up with the right answer.
Corollary: good tests are not helpful for design in the small — a good test won’t tell you the best signatures for functions.
Testing time is something worth optimizing for. Tests are sensitive to IO and IPC. Tests are relatively insensitive to the amount of code under tests.
There are useful techniques which are underused — expectation tests, coverage marks, externalized tests.
There are not so useful techniques which are over-represented in the discourse: fluent assertions, mocks, BDD.
The key for unlocking many of the above techniques is thinking in terms of data, rather than interfaces or objects.
Corollary: good tests are helpful for design in the large. They help to crystalize the data model your application is built around.
Links
https://www.destroyallsoftware.com/talks/boundaries
https://www.tedinski.com/2019/03/19/testing-at-the-boundaries.html
https://programmingisterrible.com/post/139222674273/how-to-write-disposable-code-in-large-systems
https://sans-io.readthedocs.io
https://peter.bourgon.org/blog/2021/04/02/dont-use-build-tags-for-integration-tests.html
https://buttondown.email/hillelwayne/archive/cross-branch-testing/
https://blog.janestreet.com/testing-with-expectations/
https://blog.janestreet.com/using-ascii-waveforms-to-test-hardware-designs/
https://ferrous-systems.com/blog/coverage-marks/
https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/
https://graydon2.dreamwidth.org/1597.html
https://bors.tech
https://fsharpforfunandprofit.com/posts/property-based-testing/
https://fsharpforfunandprofit.com/posts/property-based-testing-1/
https://fsharpforfunandprofit.com/posts/property-based-testing-2/
https://www.sqlite.org/testing.html
Somewhat amusingly, after writing this article I’ve learned about an excellent post by Tim Bray which argues for the opposite point:

https://www.tbray.org/ongoing/When/202x/2021/05/15/Testing-in-2021

---

Inline In Rust
Jul 9, 2021
There’s a lot of tribal knowledge surrounding #[inline] attribute in Rust. I often find myself teaching how it works, so I finally decided to write this down.

Caveat Emptor: this is what I know, not necessarily what is true. Additionally, exact semantics of #[inline] is not set in stone and may change in future Rust versions.

Why Inlining Matters?
Inlining is an optimizing transformation which replaces a call to a function with its body.

To give a trivial example, during compilation the compiler can transform this code:

```rust
fn f(w: u32) -> u32 {
    inline_me(w, 2)
}
fn inline_me(x: u32, y: u32) -> u32 {
    x * y
}
```
Into this code:

```rust
fn f(w: u32) -> u32 {
    w * 2
}
```
To paraphrase A Catalogue of Optimizing Transformations by Frances Allen and John Cocke:

There are many obvious advantages to inlining; two are:
a. There is no function call overhead whatsoever.
b. Caller and callee are optimized together. Advantage can be taken
   of particular argument values and relationships: constant arguments
   can be folded into the code, invariant instructions in the callee
   can be moved to infrequently executed areas of the caller, etc.
In other words, for an ahead of time compiled language inlining is the mother of all other optimizations. It gives the compiler the necessary context to apply further transformations.

Inlining and Separate Compilation
Inlining is at odds with another important idea in compilers — that of separate compilation. When compiling big programs, it is desirable to separate them into modules which can be compiled independently to:

Process everything in parallel.
Scope incremental recompilations to individual changed modules.
To achieve separate compilation, compilers expose signatures of functions, but keep function bodies invisible to other modules, preventing inlining. This fundamental tension is what makes #[inline] in Rust trickier than just a hint for the compiler to inline the function.

Inlining in Rust
In Rust, a unit of (separate) compilation is a crate. If a function f is defined in a crate A, then all calls to f from within A can be inlined, as the compiler has full access to f. If, however, f is called from some downstream crate B, such calls can’t be inlined. B has access only to the signature of f, not its body.

That’s where the main usage of #[inline] comes from — it enables cross-crate inlining. Without #[inline], even the most trivial of functions can’t be inlined across the crate boundary. The benefit is not without a cost — the compiler implements this by compiling a separate copy of the #[inline] function with every crate it is used in, significantly increasing compile times.

Besides #[inline], there are two more exceptions to this. Generic functions are implicitly inlinable. Indeed, the compiler can only compile a generic function when it knows the specific type arguments it is instantiated with. As that is known only in the calling crate, bodies of generic functions have to be always available.

The other exception is link-time optimization. LTO opts out of separate compilation — it makes bodies of all functions available, at the cost of making compilation much slower.

Inlining in Practice
Now that the underlying semantics is explained, it’s possible to infer some rule-of-thumbs for using #[inline].

First, it’s not a good idea to apply #[inline] indiscriminately, as that makes compile time worse. If you don’t care about compile times, a much better solution is to set lto = true in Cargo profile (docs).

Second, it usually isn’t necessary to apply #[inline] to private functions — within a crate, the compiler generally makes good inline decisions. There’s a joke that LLVM’s heuristic for when the function should be inlined is “yes”.

Third, when building an application, apply #[inline] reactively when profiling shows that a particular small function is a bottleneck. Consider using lto for releases. It might make sense to proactively #[inline] trivial public functions.

Fourth, when building libraries, proactively add #[inline] to small non-generic functions. Pay special attention to impls: Deref, AsRef and the like often benefit from inlining. A library can’t anticipate all usages upfront, it makes sense to not prematurely pessimize future users. Note that #[inline] is not transitive: if a trivial public function calls a trivial private function, you need to #[inline] both. See this benchmark for details.

Fifth, mind generic functions. It’s not too wrong to say that generic functions are implicitly inline. As a result, they often are a cause for code bloat. Generic functions, especially in libraries, should be written to minimize unwanted inlining. To give an example from wat:

```rust
// Public, generic function.
// Will cause code bloat if not handled carefully!
pub fn parse_str(wat: impl AsRef<str>) -> Result<Vec<u8>> {
  // Immediately delegate to a non-generic function.
  _parse_str(wat.as_ref())
}
// Separate-compilation friendly private implementation.
fn _parse_str(wat: &str) -> Result<Vec<u8>> {
    ...
}
```
References
Language reference.
Rust performance book.
@alexcrichton explains inline. Note that, in reality, the compile time costs are worse than what I described — inline functions are compiled per codegen-unit, not per crate.
More @alexcrichton.
Even more @alexcrichton.
Discussion on /r/rust.

There is now a follow up post: It’s Not Always iCache.

---

Large Rust Workspaces
Aug 22, 2021
In this article, I’ll share my experience with organizing large Rust projects. This is in no way authoritative — just some tips I’ve discovered through trial and error.

Cargo, Rust’s build system, follows convention over configuration principle. It provides a set of good defaults for small projects, and it is especially well-tailored for public crates.io libraries. The defaults are not perfect, but they are good enough. The resulting ecosystem-wide consistency is also welcome.

However, Cargo is less opinionated when it comes to large, multi-crate projects, organized as a Cargo workspace. Workspaces are flexible — Cargo doesn’t have a preferred layout for them. As a result, people try different things, with varying degrees of success.

To cut to the chase, I think for projects in between ten thousand and one million lines of code, the flat layout makes the most sense. rust-analyzer (200k lines) is good example here. The repository is laid out this:

```text
rust-analyzer/
  Cargo.toml
  Cargo.lock
  crates/
    rust-analyzer/
    hir/
    hir_def/
    hir_ty/
    ...
```
In the root of the repo, Cargo.toml defines a virtual manifest:

```toml
Cargo.toml
[workspace]
members = ["crates/*"]
```
Everything else (including rust-analyzer “main” crate) is nested one-level deep under crates/. The name of each directory is equal to the name of the crate:

```toml
crates/hir_def/Cargo.toml
[package]
name = "hir_def"
version = "0.0.0"
edition = "2018"
```
At the time of writing, there are 32 different subfolders in crates/.

Flat Is Better Than Nested
It’s interesting that this advice goes against the natural tendency to just organize everything hierarchically:

```text
rust-analyzer/
  Cargo.toml
  src/
  hir/
    Cargo.toml
    src/
    def/
    ty/
```
There are several reasons why trees are inferior in this case.

First, the Cargo-level namespace of crates is flat. It’s not possible to write hir::def in Cargo.toml, so crates typically have prefixes in their names. Tree layout creates an alternative hierarchy, which adds a possibility for inconsistencies.

Second, even comparatively large lists are easier to understand at a glance than even small trees. ls ./crates gives immediate bird’s eye view of the project, and this view is small enough:

```text
16:22:57|~/projects/rust-analyzer|master✓
λ ls ./crates
base_db
cfg
flycheck
hir
hir_def
hir_expand
hir_ty
ide
ide_assists
ide_completion
ide_db
ide_diagnostics
ide_ssr
limit
mbe
parser
paths
proc_macro_api
proc_macro_srv
proc_macro_test
profile
project_model
rust-analyzer
sourcegen
stdx
syntax
test_utils
text_edit
toolchain
tt
vfs
```
Doing the same for a tree-based layout is harder. Looking at a single level doesn’t tell you which folders contains nested crates. Looking at all level lists too many folders. Looking only at folder that contain Cargo.toml gives the right result, but is not as trivial as just ls.

It is true that nested structure scales better than a flat one. But the constant matters — until you hit a million lines of code, the number of crates in the project will probably fit on one screen.

Finally, the last problem with hierarchical layout is that there are no perfect hierarchies. With a flat structure, adding or splitting the crates is trivial. With a tree, you need to figure out where to put the new crate, and, if there isn’t a perfect match for it already, you’ll have to either:

add a stupid mostly empty folder near the top
add a catch-all utils folder
place the code in a known suboptimal directory.
This is a significant issue for long-lived multi-person projects — tree structure tends to deteriorate over time, while flat structure doesn’t need maintenance.

Smaller Tips
Make the root of the workspace a virtual manifest. It might be tempting to put the main crate into the root, but that pollutes the root with src/, requires passing --workspace to every Cargo command, and adds an exception to an otherwise consistent structure.

Don’t succumb to the temptation to strip common prefix from folder names. If each crate is named exactly as the folder it lives in, navigation and renames become easier. Cargo.tomls of reverse dependencies mention both the folder and the crate name, it’s useful when they are exactly the same.

For large projects a lot of repository bloat often comes from ad-hoc automation — Makefiles and various prepare.sh scripts here and there. To avoid both the bloat and proliferation of ad-hoc workflows, write all automation in Rust in a dedicated crate. One pattern useful for this is cargo xtask.

Use version = "0.0.0" for internal crates you don’t intend to publish. If you do want to publish a subset of crates with proper semver API, be very deliberate about them. It probably makes sense to extract all such crates into a separate top-level folder, libs/. It makes it easier to check that things in libs/ don’t use things from crates/.

Some crates consist only of a single-file. For those, it is tempting to flatten out the src directory and keep lib.rs and Cargo.toml in the same directory. I suggest not doing that — even if crate is single file now, it might get expanded later.

---

Fast Rust Builds
Sep 4, 2021
It’s common knowledge that Rust code is slow to compile. But I have a strong gut feeling that most Rust code out there compiles much slower than it could.

As an example, one fairly recent post says:

With Rust, on the other hand, it takes between 15 and 45 minutes to run a CI pipeline, depending on your project and the power of your CI servers.

This doesn’t make sense to me. rust-analyzer CI takes 8 minutes on GitHub actions. It is a fairly large and complex project with 200k lines of own code and 1 million lines of dependencies on top.

It is true that Rust is slow to compile in a rather fundamental way. It picked “slow compiler” in the generic dilemma, and its overall philosophy prioritizes runtime over compile time (an excellent series of posts about that: 1, 2, 3, 4). But rustc is not a slow compiler — it implements the most advanced incremental compilation in industrial compilers, it takes advantage of compilation model based on proper modules (crates), and it has been meticulously optimized. Fast to compile Rust projects are a reality, even if they are not common. Admittedly, some care and domain knowledge is required to do that.

So let’s take a closer look at what did it take for us to keep the compilation time within reasonable bounds for rust-analyzer!

Why Care About Build Times
One thing I want to make clear is that optimizing project’s build time is in some sense busy-work. Reducing compilation time provides very small direct benefits to the users, and is pure accidental complexity.

That being said, compilation time is a multiplier for basically everything. Whether you want to ship more features, to make code faster, to adapt to a change of requirements, or to attract new contributors, build time is a factor in that.

It also is a non-linear factor. Just waiting for the compiler is the smaller problem. The big one is losing the state of the flow or (worse) mental context switch to do something else while the code is compiling. One minute of work for the compiler wastes more than one minute of work for the human.

It’s hard for me to quantify the impact, but my intuitive understanding is that, as soon as the project grows beyond several thousands lines written by a single person, build times become pretty darn important!

The most devilish property of build times is that they creep up on you. While the project is small, build times are going to be acceptable. As projects grow incrementally, build times start to slowly increase as well. And if you let them grow, it might be rather hard to get them back in check later!

If project is already too slow to compile, then:

Improving build times will be time consuming, because each iteration of “try a change, trigger the build, measure improvement” will take long time (yes, build times are a multiplier for everything, including build times themselves!)
There won’t be easy wins: in contrast to runtime performance, pareto principle doesn’t work! If you write a thousand lines of code, maybe one hundred of them will be performance-sensitive, but each line will add to compile times!
Small wins will seem too small until they add up: shaving off five seconds is a much bigger deal for a five minute build than for an hour-long build.
Dually, small regressions will go unnoticed.
There’s also a culture aspect to it: if you join a project and its CI takes one hour, then an hour-long CI is normal, right?

Luckily, there’s one simple trick to solve the problem of build times …

The Silver Bullet
You need to care about build times, keep an eye on them, and fix them before they become a problem. Build times are a fairly easy optimization problem: it’s trivial to get direct feedback (just time the build), there are a bunch of tools for profiling, and you don’t even need to come up with a representative benchmark. The task is to optimize a particular project’s build time, not performance of the compiler in general. That’s a nice property of most instances of accidental complexity — they tend to be well defined engineering problems with well understood solutions.

The only hard bit about compilation time is that you don’t know that it is a problem until it actually is one! So, the most valuable thing you can get from this post is this: if you are working on a Rust project, take some time to optimize its build today, and try to repeat the exercise once in a while.

Now, with the software engineering bits cleared, let’s finally get to some actionable programming advice!

bors
I like to use CI time as one of the main metrics to keep an eye on.

Part of that is that CI time is important in itself. While you are not bound by CI when developing features, CI time directly affects how annoying it is to context switch when finishing one piece of work and starting the next one. Juggling five outstanding PRs waiting for CI to complete is not productive. Longer CI also creates a pressure to not split the work into independent chunks. If correcting a typo requires keeping a PR tab open for half a hour, it’s better to just make a drive by fix in the next feature branch, right?

But a bigger part is that CI gives you a standardized benchmark. Locally, you compile incrementally, and the time of build varies greatly with the kinds of changes you are doing. Often, you compile just a subset of the project. Due to this inherent variability, local builds give poor continuous feedback about build times. Standardized CI though runs for every change and gives you a time series where numbers are directly comparable.

To increase this standardization pressure of CI, I recommend following not rocket science rule and setting up a merge robot which guarantees that every state of the main branch passes CI. bors is a particular implementation I use, but there are others.

While it’s by far not the biggest reason to use something like bors, it gives two benefits for healthy compile times:

It ensures that every change goes via CI, and creates pressure to keep CI healthy overall
The time between leaving r+ comment on the PR and receiving the “PR merged” notification gives you an always on feedback loop. You don’t need to specifically time the build, every PR is a build benchmark.
CI Caching
If you think about it, it’s pretty obvious how a good caching strategy for CI should work. It makes sense to cache stuff that changes rarely, but it’s useless to cache frequently changing things. That is, cache all the dependencies, but don’t cache project’s own crates.

Unfortunately, almost nobody does this. A typical example would just cache the whole of ./target directory. That’s wrong — the ./target is huge, and most of it is useless on CI.

It’s not super trivial to fix though — sadly, Cargo doesn’t make it too easy to figure out which part of ./target are durable dependencies, and which parts are volatile local crates. So, you’ll need to write some code to clean the ./target before storing the cache. For GitHub actions in particular you can also use Swatinem/rust-cache.

CI Workflow
Caching is usually the low-hanging watermelon, but there are several more things to tweak.

Split CI into separate cargo test --no-run and cargo test. It is vital to know which part of your CI is the build, and which are the tests.

Disable incremental compilation. CI builds often are closer to from-scratch builds, as changes are typically much bigger than from a local edit-compile cycle. For from-scratch builds, incremental adds an extra dependency-tracking overhead. It also significantly increases the amount of IO and the size of ./target, which make caching less effective.

Disable debuginfo — it makes ./target much bigger, which again harms caching. Depending on your preferred workflow, you might consider disabling debuginfo unconditionally, this brings some benefits for local builds as well.

While we are at it, add -D warnings to the RUSTFLAGS environmental variable to deny warning for all crates at the same time. It’s a bad idea to #![deny(warnings)] in code: you need to repeat it for every crate, it needlessly makes local development harder, and it might break your users when they upgrade their compiler. It might also make sense to bump cargo network retry limits.

Read The Lockfile
Another obvious advice is to use fewer, smaller dependencies.

This is nuanced: libraries do solve actual problems, and it would be stupid to roll your own solution to something already solved by crates.io. And it’s not like it’s guaranteed that your solution will be smaller.

But it’s important to realise what problems your application is and is not solving. If you are building a CLI utility for thousands of people of to use, you absolutely need clap with all of its features. If you are writing a quick script to run during CI, which only the team will be using, it’s probably fine to start with simplistic command line parsing, but faster builds.

One tremendously useful exercise here is to read Cargo.lock (not Cargo.toml) and for each dependency think about the actual problem this dependency solves for the person in front of your application. It’s very frequent that you’ll find dependencies that just don’t make sense at all, in your context.

As an illustrative example, rust-analyzer depends on regex. This doesn’t make sense — we have exact parsers and lexers for Rust and Markdown, we don’t need to interpret regular expressions at runtime. regex is also one of the heavier dependencies — it’s a full implementation of a small language! The reason why this dependency is there is because the logging library we use allows to say something like:

RUST_LOG=rust_analyzer=very complex filtering expression
where parsing of the filtering expression is done by regular expressions.

This is undoubtedly a very useful feature to have for some applications, but in the context of rust-analyzer we don’t need it. Simple env_logger-style filtering would be enough.

Once you identify a similar redundant dependency, it’s usually enough to tweak features field somewhere, or to send a PR upstream to make non-essential bits configurable.

Sometimes it is a bigger yak to shave :) For example, rust-analyzer optionally use jemalloc crate, and its build script pulls in fs_extra and (of all the things!) paste. The ideal solution here would be of course to have a production grade, stable, pure rust memory allocator.

Profile Before Optimize
Now that we’ve dealt with things which are just sensible to do, it’s time to start measuring before cutting. A tool to use here is timings flag for Cargo (documentation). Sadly, I lack the eloquence to adequately express the level of quality and polish of this feature, so let me just say ❤️ and continue with my dry prose.

cargo build -Z timings records profiling data during the build, and then renders it as a very legible and information-dense HTML file. This is a nightly feature, so you’ll need the +nightly toggle. This isn’t a problem in practice, as you only need to run this manually once in a while.

Here’s an example from rust-analyzer:

$ cargo +nightly build -p rust-analyzer --bin rust-analyzer \
  -Z timings --release

Not only can you see how long each crate took to compile, but you’ll also see how individual compilations where scheduled, when each crate started to compile, and its critical dependency.

Compilation Model: Crates
This last point is important — crates form a directed acyclic graph of dependencies and, on a multicore CPU, the shape of this graph affects the compilation time a lot.

This is slow to compile, as all the crates need to be compiled sequentially:

A -> B -> C -> D -> E
This version is much faster, as it enables significantly more parallelism:

   +-  B  -+
  /         \
A  ->  C  ->  E
  \         /
   +-  D  -+
There’s also connection between parallelism and incrementality. In the wide graph, changing B doesn’t entail recompiling C and D.

The first advice you get when complaining about compile times in Rust is: “split the code into crates”. It is not that easy — if you ended up with a graph like the first one, you are not winning much. It is important to architect the applications to look like the second picture — a common vocabulary crate, a number of independent features, and a leaf crate to tie everything together. The most important property of a crate is which crates it doesn’t (transitively) depend on.

Another important consideration is the number of final artifacts (most typically binaries). Rust is statically linked, so, if two different binaries use the same library, each binary contains a separately linked copy of the library. If you have n binaries and m libraries, and each binary uses each library, then the amount of work to do during the linking is m * n. For this reason, it’s better to minimize the number of artifacts. One common technique here is BusyBox-style Swiss Army knife executables. The idea is that you can hardlink the same executable as several files with different names. The program then can look at the zeroth command line argument to learn the name it was invoked with, and use it effectively as a name of a subcommand. One cargo-specific gotcha here is that, by default, each file in ./examples or ./tests folder creates a new executable.

Compilation Model: Macros And Pipelining
But Cargo is even smarter than that! It does pipelined compilation — splitting the compilation of a crate into metadata and codegen phases, and starting compilation of dependent crates as soon as the metadata phase is over.

This has interesting interactions with procedural macros (and build scripts). rustc needs to run procedural macros to compute crate’s metadata. That means that procedural macros can’t be pipelined, and crates using procedural macros are blocked until the proc macro is fully compiled to the binary code.

Separately from that, procedural macros need to parse Rust code, and that is a relatively complex task. The de-facto crate for this, syn, takes quite some time to compile (not because it is bloated — just because parsing Rust is hard).

This generally means that projects tend to have syn / serde shaped hole in the CPU utilization profile during compilation. It’s relatively important to use procedural macros only where they pull their weight, and try to push crates before syn in the cargo -Z timings graph.

The latter can be tricky, as proc macro dependencies can sneak up on you. The problem here is that they are often hidden behind feature flags, and those feature flags might be enabled by downstream crates. Consider this example:

You have a convenient utility type — for example, an SSO string, in a small_string crate. To implement serialization, you don’t actually need derive (just delegating to String works), so you add an (optional) dependency on serde:

[package]
name = "small-string"
[dependencies]
serde = { version = "1" }
SSO string is a rather useful abstraction, so it gets used throughout the codebase. Then in some leaf crate which, eg, needs to expose a JSON API, you add dependency on small_string with the serde feature, as well as serde with derive itself:

[package]
name = "json-api"
[dependencies]
small-string = { version = "1", features = [ "serde" ] }
serde = { version = "1", features = [ "derive" ] }
The problem here is that json-api enables the derive feature of serde, and that means that small-string and all of its reverse-dependencies now need to wait for syn to compile! Similarly, if a crate depends on a subset of syn’s features, but something else in the crate graph enables all features, the original crate gets them as a bonus as well!

It’s not necessarily the end of the world, but it shows that dependency graph can get tricky with the presence of features. Luckily, cargo -Z timings makes it easy to notice that something strange is happening, even if it might not be always obvious what exactly went wrong.

There’s also a much more direct way for procedural macros to slow down compilation — if the macro generates a lot of code, the result would take some time to compile. That is, some macros allow you to write just a bit of source code, which feels innocuous enough, but expands to substantial amount of logic. The prime example is serialization — I’ve noticed that converting values to/from JSON accounts for surprisingly big amount of compiling. Thinking in terms of overall crate graph helps here — you want to keep serialization at the boundary of the system, in the leaf crates. If you put serialization near the foundation, then all intermediate crates would have to pay its build-time costs.

All that being said, an interesting side-note here is that procedural macros are not inherently slow to compile. Rather, it’s the fact that most proc macros need to parse Rust or to generate a lot of code that makes them slow. Sometimes, a macro can accept a simplified syntax which can be parsed without syn, and emit a tiny bit of Rust code based on that. Producing valid Rust is not nearly as complicated as parsing it!

Compilation Model: Monomorphization
Now that we’ve covered macro issues at the level of crates, it’s time to look closer, at the code-level concerns. The main thing to look here are generics. It’s vital to understand how they are compiled, which, in case of Rust, is achieved by monomorphization. Consider a run of the mill generic function:

fn frobnicate<T: SomeTrait>(x: &T) {
   ...
}
When Rust compiles this function, it doesn’t actually emit machine code. Instead, it stores an abstract representation of function body in the library. The actual compilation happens when you instantiate the function with a particular type parameter. The C++ terminology gives the right intuition here — frobnicate is a “template”, it produces an actual function when a concrete type is substituted for the parameter T.

In other words, in the following case

fn frobnicate_both(x: String, y: Widget) {
  frobnicate(&x);
  frobnicate(&y);
}
on the level of machine code there will be two separate copies of frobnicate, which would differ in details of how they deal with parameter, but would be otherwise identical.

Sounds pretty bad, right? Seems like that you can write a gigantic generic function, and then write just a small bit of code to instantiate it with a bunch of types, to create a lot of load for the compiler.

Well, I have bad news for you — the reality is much, much worse. You don’t even need different types to create duplication. Let’s say we have four crates which form a diamond

   +- B -+
  /       \
A           D
  \       /
   +- C -+
The frobnicate is defined in A, and is used by B and C

// A
pub fn frobnicate<T: SomeTrait>(x: &T) { ... }
// B
pub fn do_b(s: String) { a::frobnicate(&s) }
// C
pub fn do_c(s: String) { a::frobnicate(&s) }
// D
fn main() {
  let hello = "hello".to_owned();
  b::do_b(&hello);
  c::do_c(&hello);
}
In this case, we only ever instantiate frobincate with String, but it will get compiled twice, because monomorphization happens per crate. B and C are compiled separately, and each includes machine code for do_* functions, so they need frobnicate<String>. If optimizations are disabled, rustc can share template instantiations with dependencies, but that doesn’t work for sibling dependencies. With optimizations, rustc doesn’t share monomorphizations even with direct dependencies.

In other words, generics in Rust can lead to accidentally-quadratic compilation times across many crates!

If you are wondering whether it gets worse than that, the answer is yes. I think the actual unit of monomorphization is codegen unit, so duplicates are possible even within one crate.

Keeping an Eye on Instantiations
Besides just duplication, generics add one more problem — they shift the blame for compile times to consumers. Most of the compile time cost of generic functions is borne out by the crates that use the functionality, while the defining crate just typechecks the code without doing any code generation. Coupled with the fact that at times it is not at all obvious what gets instantiated where and why (example), this make it hard to directly see the footprint of generic APIs

Luckily, this is not needed — there’s a tool for that! cargo llvm-lines tells you which monomorphizations are happening in a specific crate.

Here’s an example from a recent investigation:

$ cargo llvm-lines --lib --release -p ide_ssr | head -n 12
 Lines          Copies        Function name
  -----          ------        -------------
  533069 (100%)  28309 (100%)  (TOTAL)
   20349 (3.8%)    357 (1.3%)  RawVec<T,A>::current_memory
   18324 (3.4%)    332 (1.2%)  <Weak<T> as Drop>::drop
   14024 (2.6%)    332 (1.2%)  Weak<T>::inner
   11718 (2.2%)    378 (1.3%)  core::ptr::metadata::from_raw_parts_mut
   10710 (2.0%)    357 (1.3%)  <RawVec<T,A> as Drop>::drop
    7984 (1.5%)    332 (1.2%)  <Arc<T> as Drop>::drop
    7968 (1.5%)    332 (1.2%)  Layout::for_value_raw
    6790 (1.3%)     97 (0.3%)  hashbrown::raw::RawTable<T,A>::drop_elements
    6596 (1.2%)     97 (0.3%)  <hashbrown::raw::RawIterRange<T> as Iterator>::next
It shows, for each generic function, how many copies of it were generated, and what’s their total size. The size is measured very coarsely, in the number of llvm ir lines it takes to encode the function. A useful fact: llvm doesn’t have generic functions, its the job of rustc to turn a function template and a set of instantiations into a set of actual functions.

Keeping Instantiations In Check
Now that we understand the pitfalls of monomorphization, a rule of thumb becomes obvious: do not put generic code at the boundaries between the crates. When designing a large system, architect it as a set of components where each of the components does something concrete and has non-generic interface.

If you do need generic interface for better type-safety and ergonomics, make sure that the interface layer is thin, and that it immediately delegates to a non-generic implementation. The classical example to internalize here are various functions from str::fs module which operate on paths:

pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
  fn inner(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
  }
  inner(path.as_ref())
}
The outer function is parameterized — it is ergonomic to use, but is compiled afresh for every downstream crate. That’s not a problem though, because it is very small, and immediately delegates to a non-generic function that gets compiled in the std.

If you are writing a function which takes a path as an argument, either use &Path, or use impl AsRef<Path> and delegate to a non-generic implementation. If you care about API ergonomics enough to use impl trait, you should use inner trick — compile times are as big part of ergonomics, as the syntax used to call the function.

A second common case here are closures: by default, prefer &dyn Fn() over impl Fn(). Similarly to paths, an impl-based nice API might be a thin wrapper around dyn-based implementation which does the bulk of the work.

Another idea along these lines is “generic, inline hotpath; concrete, outline coldpath”. In the once_cell crate, there’s this curious pattern (simplified, here’s the actual source):

struct OnceCell<T> {
  state: AtomicUsize,
  inner: Option<T>,
}
impl<T> OnceCell<T> {
  #[cold]
  fn initialize<F: FnOnce() -> T>(&self, f: F) {
    let mut f = Some(f);
    synchronize_access(self.state, &mut || {
      let f = f.take().unwrap();
      match self.inner {
        None => self.inner = Some(f()),
        Some(_value) => (),
      }
    });
  }
}
fn synchronize_access(state: &AtomicUsize, init: &mut dyn FnMut()) {
  // One hundred lines of tricky synchronization code on atomics.
}
Here, the initialize function is generic twice: first, the OnceCell is parametrized with the type of value being stored, and then initialize takes a generic closure parameter. The job of initialize is to make sure (even if it is called concurrently from many threads) that at most one f is run. This mutual exclusion task doesn’t actually depend on specific T and F and is implemented as non-generic synchronize_access, to improve compile time. One wrinkle here is that, ideally, we’d want an init: dyn FnOnce() argument, but that’s not expressible in today’s Rust. The let mut f = Some(f) / let f = f.take().unwrap() is a standard work-around for this case.

Conclusions
I guess that’s it! To repeat the main ideas:

Build times are a big factor in the overall productivity of the humans working on the project. Optimizing this is a straightforward engineering task — the tools are there. What might be hard is not letting them slowly regress. I hope this post provides enough motivation and inspiration for that! As a rough baseline, 200k line Rust project somewhat optimized for reasonable build times should take about 10 minutes of CI on GitHub actions.
