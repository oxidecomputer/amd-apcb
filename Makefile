.PHONY: all tests

CARGO = cargo

all:
	$(CARGO) build

tests:
	$(CARGO) test --tests --lib
	$(CARGO) build --features serde
	$(CARGO) build --features schemars,serde,serde-hex
	$(CARGO) build --features serde,schemars --example fromyaml
	$(CARGO) test --test '*' --features serde,schemars # integration tests only
