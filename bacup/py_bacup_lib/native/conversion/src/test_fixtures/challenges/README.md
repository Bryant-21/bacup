`python_parity.json` contains 91 input/output or input/error cases captured from
the original challenge catalog unit suite before its native migration. The
Rust test `phase::challenges::tests::matches_python_semantic_fixtures` runs the
same cases for conditions, counts, stat filters, rewards, catalog construction,
reference failures, and parent/prerequisite pruning. Non-finite input numbers
use explicit `NaN` / `Infinity` strings and exercise rejection paths.

Condition-form cycle/depth/negation tests and native plugin lookup tests live
beside the fixture runner. Python tests now cover orchestration only.
