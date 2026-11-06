# SPDX-License-Identifier: MIT OR Apache-2.0
#
# One target per check, and `check` for all of them: the single gate.
# Every target fails on a warning.

CARGO ?= cargo
MSRV  ?= 1.89.0
export RUSTFLAGS ?= -D warnings
export RUSTDOCFLAGS ?= -D warnings
# Passed through for `make msrv` on a host whose newest SDK an old
# toolchain cannot link against (see README, Development).
ifdef SDKROOT
export SDKROOT
endif

.PHONY: all check fmt fmt-fix clippy test doc deny audit msrv complexity clean

all: check

## The whole gate, fastest failures first.
check: fmt clippy complexity test doc msrv deny audit

fmt:
	$(CARGO) fmt --all -- --check

fmt-fix:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --all-targets --all-features
	$(CARGO) test --doc --all-features

doc:
	$(CARGO) doc --no-deps --all-features

## Builds the library and its tests on the declared minimum toolchain.
msrv:
	$(CARGO) +$(MSRV) check --all-targets --all-features --locked

deny:
	$(CARGO) deny --locked check

audit:
	$(CARGO) audit --deny warnings

## Per-function and per-file ceilings of ~/Code/AGENTS.md section 0.
complexity:
	python3 scripts/complexity.py

clean:
	$(CARGO) clean
