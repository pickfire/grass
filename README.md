# grass

An implementation of the SASS spec in pure Rust

To run the official test suite,

```bash
git clone https://github.com/connorskees/grass
cd grass
git submodule init
git submodule update
cargo b --release
./sass-spec/sass-spec.rb -c './target/release/grass'
```

The large features remaining are
```
special case certain functions (min, max, calc, element, expression, progid, url)
all builtin selector functions (274 tests)
content-exists, unique-id, min, min
@extend (~600 tests)
indented syntax (27 tests)
a special parser for plain css
@at-root (~150 tests)
@use and module system (~1200 tests)
@forward (~400 tests)
@keyframes (~30 tests)
@font-face (~11 tests)
string parsing/quoting/escaping (~200 tests)
```

```
2020-04-04
PASSING: 1825
FAILING: 3268
TOTAL: 5093
```

```
2020-04-01
PASSING: 1711
FAILING: 3382
TOTAL: 5093
```

```
2020-03-30
PASSING: 1685
FAILING: 3408
TOTAL: 5093
```

```
2020-03-23
PASSING: 1547
FAILING: 3546
TOTAL: 5093
```

```
2020-03-22
PASSING: 1442
FAILING: 3651
TOTAL: 5093
```

```
2020-02-24
PASSING: 1192
FAILING: 3901
TOTAL: 5093
```

```
2020-02-17
PASSING: 1115
FAILING: 3978
TOTAL: 5093
```

```
2020-02-10
PASSING: 475
FAILING: 4618
TOTAL: 5093
```

```
2020-02-03
PASSING: 242
FAILING: 4851
TOTAL: 5093
```

```
2020-01-27
PASSING: 186
FAILING: 4907
TOTAL: 5093
```

```
2020-01-20
PASSING: 143
FAILING: 4950
TOTAL: 5093
```
