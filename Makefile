.PHONY: build image usb-image qemu qemu-ui fmt clippy test check

build:
	./tools/image/build.sh

image:
	./tools/image/make_live_iso.sh

usb-image:
	./tools/image/make_usb_image.sh

qemu:
	./tools/image/run_qemu.sh

qemu-ui:
	./tools/image/run_qemu_ui.sh

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test -p abi -p openos-installer-gui

check: fmt clippy test
