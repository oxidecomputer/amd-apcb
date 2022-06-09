.PHONY: all tests

CARGO = cargo

all:
	$(CARGO) build

tests:
	$(CARGO) test
	$(CARGO) build --features serde
	$(CARGO) build --features serde,serde-hex
	$(CARGO) build --example fromyaml
