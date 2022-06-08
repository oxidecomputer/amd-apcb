.PHONY: all tests

CARGO = cargo

all:
	$(CARGO) build

tests:
	$(CARGO) test
	$(CARGO) build --features serde
	$(CARGO) build --features serde,serde-hex
	# TODO: test-compile let _foo: SerdeConfig = serde_yaml::from_str("")
