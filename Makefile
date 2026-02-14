.PHONY: build image usb-image qemu fmt clippy test check

build:
	./tools/image/build.sh

image:
	./tools/image/make_live_iso.sh

usb-image:
	./tools/image/make_usb_image.sh

qemu:
	./tools/image/run_qemu.sh

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test -p abi -p openos-installer-gui

check: fmt clippy test
